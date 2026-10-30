//! Left pane strip and pane contents.

use dioxus::html::input_data::MouseButton;
use dioxus::html::{InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction};
use dioxus::prelude::*;
use folitecta_core::commands::PANES;
use folitecta_core::search::FoundIn;
use folitecta_core::{Command, registry};

use crate::commands::{dispatch, option_on};
use crate::files;
use crate::i18n::{cmd_label, t, tf, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Pane, ToastKind};
use crate::ui::{MENU_W, status_badge};

mod search;
mod thumbnails;

pub mod accessibility;
pub mod attachments;
pub mod bookmarks;
pub mod comment_styles;
pub mod comments;
pub mod explorer;
pub mod fields;
pub mod history;
pub mod layers;
pub mod right;

pub use search::*;
pub(crate) use thumbnails::thumb_render_scale;
use thumbnails::*;

fn pane_icon(p: &Pane) -> &'static str {
    match p {
        Pane::Thumbnails => "pages",
        Pane::Bookmarks => "bookmark",
        Pane::Comments => "comment",
        Pane::Attachments => "attachment",
        Pane::Layers => "layers",
        Pane::Fields => "field",
        Pane::Links => "link",
        Pane::Destinations => "star",
        Pane::Search => "search",
        Pane::Signatures => "signature",
        Pane::History => "history",
        Pane::Sessions => "window",
        Pane::Stamps => "tag",
        Pane::Comparison => "swap",
        Pane::Tags => "tags",
        Pane::Order => "order",
        Pane::Content => "content",
        Pane::AccChecker => "accessibility",
        Pane::AccReport => "check",
        Pane::Explorer => "folder",
        Pane::Other(_) => "grid",
    }
}

fn pane_label(st: AppState, p: &Pane) -> String {
    let fallback = PANES
        .iter()
        .find(|(id, _)| *id == p.id())
        .map(|(_, l)| *l)
        .unwrap_or("Pane");
    ui_label(st.lang(), "pane", &p.id(), fallback)
}

// ---- Search -------------------------------------------------------------------

#[component]
pub fn PaneStrip() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (show, current, ribbon) = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone(), ui.ribbon)
    };
    let more_open = (st.open_dropdown)().as_deref() == Some("panes:more");
    // Main pane buttons that fit the strip height; the rest move to the "more" menu.
    let (_, vh) = (st.viewport)();
    let chrome = 31.0 + if ribbon { 132.0 } else { 40.0 } + 30.0 + 14.0;
    let slots =
        (((vh - chrome - 6.0) / 36.0).floor() as isize - 2).clamp(3, Pane::MAIN.len() as isize) as usize;
    let (shown, hidden) = Pane::MAIN.split_at(slots);
    let others: Vec<(&str, &str)> = PANES
        .iter()
        .filter(|(id, _)| !Pane::MAIN.iter().any(|m| m.id() == *id) && *id != "properties")
        .copied()
        .collect();
    let show_right = st.ui.read().show_right_pane;
    let (mx, my) = (st.dropdown_pos)();
    let max_h = ((st.viewport)().1 - my - 8.0).max(160.0);
    let hidden_active = hidden.iter().any(|p| show && p.id() == current);
    rsx! {
        div { class: "pane-strip", onclick: move |e| e.stop_propagation(),
            for pane in shown.iter() {
                {
                    let id = pane.id();
                    let active = show && current == id;
                    let label = pane_label(st, pane);
                    let id_click = id.clone();
                    rsx! {
                        button { class: if active { "strip-btn active" } else { "strip-btn" }, key: "{id}", title: "{label}",
                            onclick: move |_| {
                                let mut ui = st.ui_mut();
                                if ui.show_left_pane && ui.left_pane == id_click { ui.show_left_pane = false; } else { ui.show_left_pane = true; ui.left_pane = id_click.clone(); }
                                drop(ui);
                                st.save_prefs();
                            },
                            Icon { name: pane_icon(pane) }
                        }
                    }
                }
            }
            button { class: if more_open || hidden_active { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.more\")}",
                onclick: move |evt| {
                    let cur = st.open_dropdown.peek().clone();
                    if cur.as_deref() == Some("panes:more") {
                        st.set_open_dropdown(None);
                    } else {
                        let c = evt.client_coordinates();
                        let o = evt.element_coordinates();
                        let (left, top) = (c.x - o.x + 40.0, c.y - o.y);
                        let pos = st.clamp_menu_pos(left, top, MENU_W, 260.0);
                        st.set_dropdown_pos(pos);
                        st.set_open_dropdown(Some("panes:more".into()));
                    }
                },
                Icon { name: "more" }
            }
            if more_open {
                div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px;max-height:{max_h}px",
                    for pane in hidden.iter() {
                        {
                            let id_s = pane.id();
                            let label = pane_label(st, pane);
                            let active = show && current == id_s;
                            rsx! {
                                div { class: if active { "menu-item open" } else { "menu-item" }, key: "{id_s}",
                                    onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                    Icon { name: pane_icon(pane) }
                                    span { class: "label", "{label}" }
                                }
                            }
                        }
                    }
                    if !hidden.is_empty() { div { class: "menu-sep" } }
                    for (id, label) in others {
                        {
                            let id_s = id.to_string();
                            let shown = ui_label(lang, "pane", id, label);
                            let pane = Pane::from_id(id);
                            let planned = matches!(pane, Pane::Other(_));
                            rsx! {
                                div { class: "menu-item", key: "{id}",
                                    onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                    if !planned { Icon { name: pane_icon(&pane) } }
                                    span { class: "label", "{shown}" }
                                    if planned { span { class: "badge planned", "{t(lang, \"label.planned\").to_lowercase()}" } }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "strip-spacer" }
            button { class: if show_right { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.properties\")}",
                onclick: move |_| { let v = !st.ui.read().show_right_pane; st.ui_mut().show_right_pane = v; st.save_prefs(); },
                Icon { name: "info" }
            }
        }
    }
}

/// Registry commands of a pane, shown in the pane's options menu.
fn pane_commands(pane_id: &str) -> Vec<&'static Command> {
    registry().pane_commands(pane_id)
}

#[component]
pub fn PanePanel() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (show, current) = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    if !show {
        return rsx! { div { class: "pane-panel" } };
    }
    let pane = Pane::from_id(&current);
    let label = pane_label(st, &pane);
    let has_doc = st.has_doc();
    let menu_key = format!("pane-menu:{current}");
    let menu_open = (st.open_dropdown)().as_deref() == Some(menu_key.as_str());
    let cmds = if matches!(pane, Pane::Other(_)) {
        Vec::new()
    } else {
        pane_commands(&current)
    };
    let (mx, my) = (st.dropdown_pos)();
    let max_h = ((st.viewport)().1 - my - 8.0).max(160.0);
    // History and sessions work without a document.
    let body = if pane == Pane::History {
        rsx! { crate::ui::panes::history::HistoryPane {} }
    } else if pane == Pane::Sessions {
        rsx! { crate::ui::panes::history::SessionsPane {} }
    } else if pane == Pane::Stamps {
        rsx! { crate::ui::features::stamps::StampsPane {} }
    } else if pane == Pane::Explorer {
        rsx! { crate::ui::panes::explorer::ExplorerPane {} }
    } else if pane == Pane::Other("comment-styles".into()) {
        rsx! { crate::ui::panes::comment_styles::CommentStylesPane {} }
    } else if !has_doc {
        rsx! { div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" } }
    } else {
        match &pane {
            Pane::Thumbnails => rsx! { ThumbnailsPane {} },
            Pane::Bookmarks => rsx! { crate::ui::panes::bookmarks::BookmarksPane {} },
            Pane::Comments => rsx! { crate::ui::panes::comments::CommentsPane {} },
            Pane::Attachments => rsx! { crate::ui::panes::attachments::AttachmentsPane {} },
            Pane::Layers => rsx! { crate::ui::panes::layers::LayersPane {} },
            Pane::Fields => rsx! { crate::ui::panes::fields::FieldsPane {} },
            Pane::Links => rsx! { LinksPane {} },
            Pane::Destinations => rsx! { crate::ui::panes::bookmarks::DestinationsPane {} },
            Pane::Search => rsx! { SearchPane {} },
            Pane::Signatures => rsx! { crate::ui::features::signatures::SignaturesPane {} },
            Pane::Comparison => rsx! { crate::ui::features::compare::ComparisonPane {} },
            Pane::Tags => rsx! { crate::ui::panes::accessibility::TagsPane {} },
            Pane::Order => rsx! { crate::ui::panes::accessibility::OrderPane {} },
            Pane::Content => rsx! { crate::ui::panes::accessibility::ContentPane {} },
            Pane::AccChecker => rsx! { crate::ui::panes::accessibility::CheckerPane {} },
            Pane::AccReport => rsx! { crate::ui::panes::accessibility::ReportPane {} },
            Pane::History | Pane::Sessions | Pane::Stamps | Pane::Explorer => rsx! {},
            Pane::Other(id) => rsx! { PlannedPane { pane_id: id.clone() } },
        }
    };
    let key_click = menu_key.clone();
    rsx! {
        div { class: "pane-panel", onclick: move |e| e.stop_propagation(),
            div { class: "pane-header", Icon { name: pane_icon(&pane) } span { class: "pane-title", "{label}" } div { class: "spacer" }
                if has_doc && !cmds.is_empty() {
                    button { class: if menu_open { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"pane.options\")}",
                        onclick: move |evt| {
                            if st.open_dropdown.peek().as_deref() == Some(key_click.as_str()) {
                                st.set_open_dropdown(None);
                            } else {
                                let c = evt.client_coordinates();
                                let o = evt.element_coordinates();
                                let pos = st.clamp_menu_pos(c.x - o.x, c.y - o.y + 28.0, MENU_W, 300.0);
                                st.set_dropdown_pos(pos);
                                st.submenus_mut().clear();
                                st.set_open_dropdown(Some(key_click.clone()));
                            }
                        },
                        Icon { name: "more" }
                    }
                }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| { st.ui_mut().show_left_pane = false; st.save_prefs(); }, "×" }
            }
            if menu_open {
                div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px;max-height:{max_h}px", onclick: move |e| e.stop_propagation(),
                    for c in cmds {
                        {crate::ui::menu_entry(st, lang, c, 0)}
                    }
                }
            }
            div { class: "pane-body", {body} }
        }
    }
}
// ---- Thumbnails -------------------------------------------------------------------

/// The links of the document: a click selects one and shows it on its page
/// (the pane menu copies or opens its address), a double click follows it.
#[component]
fn LinksPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((links, selected)) = st.read_active(|t| (t.doc.all_links(), t.selected_link)) else {
        return rsx! {};
    };
    let wrap = crate::commands::option_on(st, "linksView.wrapTitles");
    rsx! {
        if links.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for l in links {
            {
                let target = l.target.clone();
                let (page, index, rect) = (l.page, l.index, l.rect);
                let desc = target.describe();
                let class = if selected == Some((page, index)) { "list-item selected" } else { "list-item" };
                rsx! {
                    div { class: "{class}", key: "{page}-{index}",
                        onclick: move |_| {
                            st.go_to_page(page);
                            st.with_active(|t| {
                                t.selected_link = Some((page, index));
                                if let Some(size) = t.page_sizes.get(page) {
                                    t.flash = Some((page, folitecta_core::marks::pdf_rect_to_display(size, rect)));
                                }
                            });
                        },
                        ondoubleclick: move |_| {
                            crate::commands::navigation::follow::follow(st, target.clone());
                        },
                        div { class: "title", Icon { name: "link" } "{t(lang, \"status.page\")} {page + 1}" }
                        div { class: if wrap { "body wrap" } else { "body ellipsis" }, title: "{desc}", "{desc}" }
                    }
                }
            }
        }
    }
}

// ---- Search pane ------------------------------------------------------------------

#[component]
fn PlannedPane(pane_id: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let cmds = reg.pane_commands(&pane_id);
    rsx! {
        div { class: "callout", style: "margin: 6px", "{t(lang, \"pane.planned\")}" }
        for c in cmds {
            {
                let id = c.id.clone();
                let label = cmd_label(lang, c);
                rsx! {
                    div { class: "tree-item", key: "{c.id}", onclick: move |_| dispatch(st, &id),
                        span { class: "tree-label", "{label}" }
                        {status_badge(&c.id)}
                    }
                }
            }
        }
    }
}
