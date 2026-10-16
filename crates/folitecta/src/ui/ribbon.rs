//! Ribbon (tabs, groups, buttons) and the compact classic toolbar.

use dioxus::prelude::*;
use folitecta_core::commands::{RIBBON_TABS, Registry};
use folitecta_core::{Command, registry};

use crate::commands::{self, is_implemented};
use crate::i18n::{Lang, cmd_label, t, ui_label};
use crate::icons::{self, Icon};
use crate::state::{AppState, Layout, Pane, Theme, Tool, Zoom};
use crate::ui::menu_entry;

const MAX_LARGE_BUTTONS: usize = 3;

/// Whether a toggle-like command is currently "on".
fn is_active(st: AppState, id: &str) -> bool {
    let tool = (st.tool)();
    if tool.command_id() == id
        || (id == "addStickyNote" && tool == Tool::Note)
        || (id == "addLink" && tool == Tool::Link)
    {
        return true;
    }
    let ui = st.ui.read();
    let pane_on = |p: Pane| ui.show_left_pane && ui.left_pane == p.id();
    match id {
        "view.pageThumbnails" => pane_on(Pane::Thumbnails),
        "view.bookmarks" => pane_on(Pane::Bookmarks),
        "view.comments" => pane_on(Pane::Comments),
        "view.attachments" => pane_on(Pane::Attachments),
        "view.layers" => pane_on(Pane::Layers),
        "view.fields" => pane_on(Pane::Fields),
        "view.links" => pane_on(Pane::Links),
        "view.destinations" => pane_on(Pane::Destinations),
        "view.signatures" => pane_on(Pane::Signatures),
        "view.search" | "find" | "search" => pane_on(Pane::Search),
        "view.properties" => ui.show_right_pane,
        "view.darkPageMode" | "view.theme.darkGray" | "view.theme.black" => ui.theme == Theme::Dark,
        "view.theme.white" | "view.theme.default" => ui.theme == Theme::Light,
        "view.switchToRibbonUI" => ui.ribbon,
        "view.switchToClassicUI" => !ui.ribbon,
        "view.fullScreen" => (st.fullscreen)(),
        "view.layout.onePage" => st.read_active(|t| t.layout == Layout::Single).unwrap_or(false),
        "view.layout.oneColumn" => st
            .read_active(|t| t.layout == Layout::Continuous)
            .unwrap_or(false),
        "view.layout.twoPages" | "view.layout.twoColumns" => {
            st.read_active(|t| t.layout == Layout::TwoPages).unwrap_or(false)
        }
        "zoom.level.fitWidth" => st.read_active(|t| t.zoom == Zoom::FitWidth).unwrap_or(false),
        "zoom.level.fitPage" => st.read_active(|t| t.zoom == Zoom::FitPage).unwrap_or(false),
        "forms.highlight" | "forms.highlight.all" => st.read_active(|t| t.highlight_fields).unwrap_or(false),
        _ => false,
    }
}

fn ribbon_button(
    st: AppState,
    lang: Lang,
    reg: &'static Registry,
    cmd: &'static Command,
    small: bool,
) -> Element {
    let id = cmd.id.clone();
    let id_click = id.clone();
    let label = cmd_label(lang, cmd);
    let has_menu = reg.has_children(&cmd.id);
    let icon = icons::for_command(&cmd.id);
    let active = is_active(st, &cmd.id);
    let planned = !is_implemented(&cmd.id) && !has_menu;
    let open = (st.open_dropdown)().as_deref() == Some(cmd.id.as_str());
    let mut class = String::from("rbtn");
    if small {
        class.push_str(" small");
    }
    if active {
        class.push_str(" active");
    }
    if has_menu {
        class.push_str(" has-menu");
    }
    let tip = if planned {
        format!("{label} — {}", t(lang, "label.planned"))
    } else {
        label.clone()
    };
    rsx! {
        div { class: "rbtn-wrap", key: "{cmd.id}",
            button {
                class: "{class}",
                title: "{tip}",
                onclick: move |e| {
                    e.stop_propagation();
                    if has_menu {
                        let cur = st.open_dropdown.peek().clone();
                        st.set_open_dropdown(if cur.as_deref() == Some(id_click.as_str()) { None } else { Some(id_click.clone()) });
                        st.set_open_menu(None);
                    } else {
                        commands::dispatch(st, &id_click);
                    }
                },
                Icon { name: icon }
                span { class: "lbl", "{label}" }
                if planned { span { class: "dot" } }
            }
            if has_menu && open {
                div { class: "rbtn-menu", onclick: move |e| e.stop_propagation(),
                    for child in reg.children(&id) {
                        {menu_entry(st, lang, child, 1)}
                    }
                }
            }
        }
    }
}

fn more_button(st: AppState, lang: Lang, key: String, rest: Vec<&'static Command>) -> Element {
    let open = (st.open_dropdown)().as_deref() == Some(key.as_str());
    let key_click = key.clone();
    let count = rest.len();
    rsx! {
        div { class: "rbtn-wrap", key: "{key}",
            button { class: "rbtn has-menu", title: "{count} more commands",
                onclick: move |e| {
                    e.stop_propagation();
                    let cur = st.open_dropdown.peek().clone();
                    st.set_open_dropdown(if cur.as_deref() == Some(key_click.as_str()) { None } else { Some(key_click.clone()) });
                },
                Icon { name: "more" }
                span { class: "lbl", "More ({count})" }
            }
            if open {
                div { class: "rbtn-menu", style: "min-width:300px", onclick: move |e| e.stop_propagation(),
                    for c in rest {
                        {menu_entry(st, lang, c, 1)}
                    }
                }
            }
        }
    }
}

#[component]
pub fn Ribbon() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let active_tab = (st.ribbon_tab)();
    let tool = (st.tool)();
    let ctx_tab: Option<&'static str> = if tool.is_drawing() {
        Some("ctx.format")
    } else {
        None
    };
    let tab_for_groups = active_tab.clone();

    rsx! {
        div { class: "ribbon", onclick: move |e| e.stop_propagation(),
            div { class: "ribbon-tabs",
                for (id, label) in RIBBON_TABS.iter() {
                    {
                        let id_s = id.to_string();
                        let title = ui_label(lang, "tab", id, label);
                        let is_active = active_tab == *id;
                        rsx! {
                            button { class: if is_active { "ribbon-tab active" } else { "ribbon-tab" }, key: "{id}",
                                onclick: move |_| { st.set_ribbon_tab(id_s.clone()); st.set_open_dropdown(None); },
                                "{title}"
                            }
                        }
                    }
                }
                if let Some(ctx) = ctx_tab {
                    button { class: if active_tab == ctx { "ribbon-tab ctx active" } else { "ribbon-tab ctx" },
                        onclick: move |_| st.set_ribbon_tab(ctx.to_string()),
                        "{Registry::tab_label(ctx)}"
                    }
                }
                div { class: "spacer" }
                div { class: "ribbon-right",
                    button { class: "tbtn", onclick: move |_| commands::dispatch(st, "open"), Icon { name: "open" } "{cmd_label(lang, reg.get(\"open\").unwrap())}" }
                    button { class: "tbtn", onclick: move |_| commands::dispatch(st, "save"), Icon { name: "save" } "{cmd_label(lang, reg.get(\"save\").unwrap())}" }
                    button { class: "ibtn", title: "Undo (Ctrl+Z)", onclick: move |_| commands::dispatch(st, "undo"), Icon { name: "undo" } }
                    button { class: "ibtn", title: "Redo (Ctrl+Y)", onclick: move |_| commands::dispatch(st, "redo"), Icon { name: "redo" } }
                }
            }
            div { class: "ribbon-body",
                for (group, group_label) in reg.ribbon_groups(&tab_for_groups) {
                    {
                        let cmds: Vec<&'static Command> = reg.ribbon_group(&tab_for_groups, group);
                        let (shown, rest): (Vec<&'static Command>, Vec<&'static Command>) = if cmds.len() > MAX_LARGE_BUTTONS {
                            let (a, b) = cmds.split_at(MAX_LARGE_BUTTONS - 1);
                            (a.to_vec(), b.to_vec())
                        } else {
                            (cmds.clone(), Vec::new())
                        };
                        let more_key = format!("more:{tab_for_groups}:{group}");
                        rsx! {
                            div { class: "ribbon-group", key: "{group}",
                                div { class: "ribbon-group-items",
                                    for c in shown {
                                        {ribbon_button(st, lang, reg, c, false)}
                                    }
                                    if !rest.is_empty() {
                                        {more_button(st, lang, more_key, rest)}
                                    }
                                }
                                div { class: "ribbon-group-title", "{group_label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compact toolbar used by the classic (non-ribbon) interface.
#[component]
pub fn ClassicToolbar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let ids = [
        "open",
        "save",
        "print",
        "-",
        "undo",
        "redo",
        "-",
        "tool.hand",
        "tool.selectText",
        "tool.zoom",
        "-",
        "view.goto.prevPage",
        "view.goto.nextPage",
        "-",
        "zoomOut",
        "zoomIn",
        "zoom.level.fitWidth",
        "zoom.level.fitPage",
        "-",
        "view.rotate.CCW",
        "view.rotate.CW",
        "-",
        "addStickyNote",
        "tool.annot.square",
        "tool.annot.highlightArea",
        "tool.annot.textBox",
        "-",
        "document.rotatePages",
        "document.deletePages",
        "document.insertEmptyPages",
        "document.watermarks.add",
    ];
    rsx! {
        div { class: "ribbon", onclick: move |e| e.stop_propagation(),
            div { class: "ribbon-body", style: "min-height:0; padding: 4px 8px; gap: 2px; align-items: center",
                for (i, id) in ids.iter().enumerate() {
                    if *id == "-" {
                        div { key: "sep{i}", style: "width:1px;height:22px;background:var(--border);margin:0 4px" }
                    } else if let Some(cmd) = reg.get(id) {
                        {ribbon_button(st, lang, reg, cmd, true)}
                    }
                }
            }
        }
    }
}
