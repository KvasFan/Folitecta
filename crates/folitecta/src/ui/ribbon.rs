//! Ribbon (tabs, groups, buttons) and the compact classic toolbar.

use dioxus::html::{InteractionElementOffset, InteractionLocation};
use dioxus::prelude::*;
use folitecta_core::{Command, registry};

use crate::commands::{self, is_implemented};
use crate::i18n::{Lang, cmd_label, t, ui_label};
use crate::icons::{self, Icon};
use crate::state::{AppState, Layout, Pane, Theme, Tool, Zoom};
use crate::ui::menu_entry;
use crate::ui::ribbon_layout::{self, CONTEXT_TAB, GroupSpec, RGroup, fit_ribbon, large_width, small_width};

const LARGE_H: f64 = 62.0;
const SMALL_H: f64 = 20.0;
const ICON_H: f64 = 28.0;
const MENU_W: f64 = 300.0;

/// How a ribbon command is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnStyle {
    /// Icon above a two-line label (62 x 62).
    Large,
    /// Icon and label in one row, stacked three per column.
    Compact,
    /// Icon only (classic toolbar).
    Icon,
}

impl BtnStyle {
    fn class(self) -> &'static str {
        match self {
            BtnStyle::Large => "rbtn",
            BtnStyle::Compact => "rbtn compact",
            BtnStyle::Icon => "rbtn small",
        }
    }
    fn height(self) -> f64 {
        match self {
            BtnStyle::Large => LARGE_H,
            BtnStyle::Compact => SMALL_H,
            BtnStyle::Icon => ICON_H,
        }
    }
}

/// Whether a toggle-like command is currently "on".
pub fn is_active(st: AppState, id: &str) -> bool {
    let tool = (st.tool)();
    if tool.command_id() == id
        || (id == "addStickyNote" && tool == Tool::Note)
        || (id == "addLink" && tool == Tool::Link)
    {
        return true;
    }
    if let Some(v) = crate::commands::toggle_state(st, id) {
        return v;
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

/// Viewport position just below the element that received `evt`.
/// Children of ribbon buttons ignore pointer events, so the target is the button.
fn anchor_below(evt: &Event<MouseData>, height: f64) -> (f64, f64) {
    let c = evt.client_coordinates();
    let o = evt.element_coordinates();
    (c.x - o.x, c.y - o.y + height + 2.0)
}

/// A dropdown list anchored at `st.dropdown_pos`, kept inside the window.
pub fn dropdown(st: AppState, lang: Lang, cmds: Vec<&'static Command>) -> Element {
    let (x, y) = (st.dropdown_pos)();
    let (vw, vh) = (st.viewport)();
    let left = x.min(vw - MENU_W - 8.0).max(4.0);
    let room = vh - y - 10.0;
    let (top, max_h) = if room < 180.0 {
        ((vh - 260.0).max(4.0), 250.0)
    } else {
        (y, room)
    };
    rsx! {
        div { class: "rbtn-menu", style: "left:{left}px;top:{top}px;max-height:{max_h}px",
            onclick: move |e| e.stop_propagation(),
            for c in cmds {
                {menu_entry(st, lang, c, 0)}
            }
        }
    }
}

fn toggle_dropdown(st: AppState, key: &str, pos: (f64, f64)) {
    let cur = st.open_dropdown.peek().clone();
    if cur.as_deref() == Some(key) {
        st.set_open_dropdown(None);
    } else {
        st.set_dropdown_pos(pos);
        st.submenus_mut().clear();
        st.set_open_dropdown(Some(key.to_string()));
    }
    st.set_open_menu(None);
}

pub fn ribbon_button(st: AppState, lang: Lang, cmd: &'static Command, style: BtnStyle) -> Element {
    let reg = registry();
    let id = cmd.id.clone();
    let id_click = id.clone();
    let label = cmd_label(lang, cmd);
    let has_menu = reg.has_children(&cmd.id) && !is_implemented(&cmd.id);
    let icon = icons::for_command(&cmd.id);
    let active = is_active(st, &cmd.id);
    let planned = !is_implemented(&cmd.id) && !has_menu;
    let key = format!("btn:{}", cmd.id);
    let open = (st.open_dropdown)().as_deref() == Some(key.as_str());
    let mut class = String::from(style.class());
    if active {
        class.push_str(" active");
    }
    if has_menu {
        class.push_str(" has-menu");
    }
    if planned {
        class.push_str(" planned");
    }
    let tip = match (planned, cmd.shortcut.as_deref()) {
        (true, _) => format!("{label} — {}", t(lang, "label.planned")),
        (false, Some(s)) => format!("{label} ({s})"),
        (false, None) => label.clone(),
    };
    let height = style.height();
    let width_style = if style == BtnStyle::Large {
        format!("width:{}px", large_width(&label) - 2.0)
    } else {
        String::new()
    };
    rsx! {
        div { class: "rbtn-wrap", key: "{cmd.id}",
            button {
                class: "{class}",
                style: "{width_style}",
                title: "{tip}",
                onclick: move |e| {
                    e.stop_propagation();
                    if has_menu {
                        toggle_dropdown(st, &key, anchor_below(&e, height));
                    } else {
                        commands::dispatch(st, &id_click);
                    }
                },
                Icon { name: icon }
                span { class: "lbl", "{label}" }
                if planned { span { class: "dot" } }
            }
            if has_menu && open {
                {dropdown(st, lang, reg.children(&id))}
            }
        }
    }
}

/// Overflow button listing the commands that did not fit (or a collapsed group).
fn overflow_button(
    st: AppState,
    lang: Lang,
    key: String,
    label: String,
    icon: &'static str,
    cmds: Vec<&'static Command>,
    style: BtnStyle,
) -> Element {
    let open = (st.open_dropdown)().as_deref() == Some(key.as_str());
    let key_click = key.clone();
    let count = cmds.len();
    let class = format!("{} has-menu", style.class());
    let height = style.height();
    let width_style = if style == BtnStyle::Large {
        format!("width:{}px", large_width(&label) - 2.0)
    } else {
        String::new()
    };
    rsx! {
        div { class: "rbtn-wrap", key: "{key}",
            button { class: "{class}", style: "{width_style}", title: "{label} ({count})",
                onclick: move |e| {
                    e.stop_propagation();
                    toggle_dropdown(st, &key_click, anchor_below(&e, height));
                },
                Icon { name: icon }
                span { class: "lbl", "{label}" }
            }
            if open {
                {dropdown(st, lang, cmds)}
            }
        }
    }
}

fn group_label(lang: Lang, tab: &str, g: &RGroup) -> String {
    ui_label(lang, "group", &format!("{tab}.{}", g.id), g.label)
}

fn title_width(label: &str) -> f64 {
    label.chars().count() as f64 * 6.6 + 12.0
}

#[component]
pub fn Ribbon() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let active_tab = (st.ribbon_tab)();
    let (vw, _) = (st.viewport)();
    let tool = (st.tool)();
    let show_ctx = tool.is_drawing() || st.read_active(|t| t.selected_annot.is_some()).unwrap_or(false);
    let tab = ribbon_layout::tab(&active_tab)
        .filter(|t| show_ctx || !t.id.starts_with("ctx."))
        .unwrap_or(&ribbon_layout::RIBBON[0]);
    let tab_id = tab.id;

    let groups: Vec<(&'static RGroup, Vec<&'static Command>, Vec<&'static Command>)> = tab
        .groups
        .iter()
        .map(|g| {
            let items: Vec<&'static Command> = g.items.iter().filter_map(|id| reg.get(id)).collect();
            let extras: Vec<&'static Command> = reg
                .ribbon_group(tab_id, g.id)
                .into_iter()
                .filter(|c| !g.items.contains(&c.id.as_str()))
                .collect();
            (g, items, extras)
        })
        .collect();
    let more_w = small_width(&format!("{} (99)", t(lang, "label.more")));
    let specs: Vec<GroupSpec> = groups
        .iter()
        .map(|(g, items, extras)| GroupSpec {
            small_w: items.iter().map(|c| small_width(&cmd_label(lang, c))).collect(),
            large_w: items.iter().map(|c| large_width(&cmd_label(lang, c))).collect(),
            extra: !extras.is_empty(),
            title: title_width(&group_label(lang, tab_id, g)),
            more_w,
            collapsed_w: large_width(&group_label(lang, tab_id, g)),
        })
        .collect();
    let fits = fit_ribbon(&specs, vw - 28.0);
    let open_label = cmd_label(lang, reg.get("open").unwrap());
    let save_label = cmd_label(lang, reg.get("save").unwrap());
    let undo_label = cmd_label(lang, reg.get("undo").unwrap());
    let redo_label = cmd_label(lang, reg.get("redo").unwrap());

    rsx! {
        div { class: "ribbon", onclick: move |e| e.stop_propagation(),
            div { class: "ribbon-tabs",
                for t in ribbon_layout::main_tabs() {
                    {
                        let id = t.id;
                        let title = ui_label(lang, "tab", id, t.label);
                        rsx! {
                            button { class: if tab_id == id { "ribbon-tab active" } else { "ribbon-tab" }, key: "{id}", title: "{title}",
                                onclick: move |_| { st.set_ribbon_tab(id.to_string()); st.set_open_dropdown(None); },
                                "{title}"
                            }
                        }
                    }
                }
                if show_ctx {
                    {
                        let ctx = ribbon_layout::tab(CONTEXT_TAB).unwrap();
                        let title = ui_label(lang, "tab", ctx.id, ctx.label);
                        rsx! {
                            button { class: if tab_id == CONTEXT_TAB { "ribbon-tab ctx active" } else { "ribbon-tab ctx" }, title: "{title}",
                                onclick: move |_| st.set_ribbon_tab(CONTEXT_TAB.to_string()),
                                "{title}"
                            }
                        }
                    }
                }
                div { class: "spacer" }
                div { class: "ribbon-right",
                    button { class: "tbtn", title: "{open_label} (Ctrl+O)", onclick: move |_| commands::dispatch(st, "open"), Icon { name: "open" } span { class: "txt", "{open_label}" } }
                    button { class: "tbtn", title: "{save_label} (Ctrl+S)", onclick: move |_| commands::dispatch(st, "save"), Icon { name: "save" } span { class: "txt", "{save_label}" } }
                    button { class: "ibtn", title: "{undo_label} (Ctrl+Z)", onclick: move |_| commands::dispatch(st, "undo"), Icon { name: "undo" } }
                    button { class: "ibtn", title: "{redo_label} (Ctrl+Y)", onclick: move |_| commands::dispatch(st, "redo"), Icon { name: "redo" } }
                }
            }
            div { class: "ribbon-body",
                for (i, (g, items, extras)) in groups.into_iter().enumerate() {
                    {
                        let (large, small) = fits.get(i).copied().unwrap_or((0, 0));
                        let large = large.min(items.len());
                        let small = small.min(items.len() - large);
                        let shown = large + small;
                        let label = group_label(lang, tab_id, g);
                        let mut overflow: Vec<&'static Command> = items[shown..].to_vec();
                        overflow.extend(extras);
                        let more_key = format!("more:{tab_id}:{}", g.id);
                        let collapsed = shown == 0;
                        let more_label = if collapsed { label.clone() } else { format!("{} ({})", t(lang, "label.more"), overflow.len()) };
                        let more_icon = if collapsed { icons::for_command(g.items.first().copied().unwrap_or("more")) } else { "more" };
                        // Small buttons stack three per column; the overflow button joins the last column.
                        let mut stack: Vec<Option<&'static Command>> = items[large..shown].iter().map(|c| Some(*c)).collect();
                        let more_in_stack = small > 0 && !overflow.is_empty();
                        if more_in_stack {
                            stack.push(None);
                        }
                        let columns: Vec<Vec<Option<&'static Command>>> = stack.chunks(3).map(|c| c.to_vec()).collect();
                        rsx! {
                            div { class: "ribbon-group", key: "{tab_id}-{g.id}",
                                div { class: "ribbon-group-items",
                                    for c in items[..large].iter() {
                                        {ribbon_button(st, lang, c, BtnStyle::Large)}
                                    }
                                    for (ci, col) in columns.into_iter().enumerate() {
                                        div { class: "rb-col", key: "col{ci}",
                                            for entry in col {
                                                match entry {
                                                    Some(c) => ribbon_button(st, lang, c, BtnStyle::Compact),
                                                    None => overflow_button(st, lang, more_key.clone(), more_label.clone(), more_icon, overflow.clone(), BtnStyle::Compact),
                                                }
                                            }
                                        }
                                    }
                                    if !overflow.is_empty() && !more_in_stack {
                                        {overflow_button(st, lang, more_key.clone(), more_label.clone(), more_icon, overflow.clone(), BtnStyle::Large)}
                                    }
                                }
                                div { class: "ribbon-group-title", "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
