//! Classic menu bar built from the command registry.

use dioxus::prelude::*;
use folitecta_core::commands::MENUS;
use folitecta_core::registry;

use crate::app::icon_data_url;
use crate::i18n::{Lang, t, ui_label};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::menu_entry;

#[component]
pub fn Menubar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let open = (st.open_menu)();
    let reg = registry();
    let icon = icon_data_url();
    let theme = st.ui.read().theme;
    let theme_icon: &'static str = if theme.is_dark() { "sun" } else { "moon" };
    // A hidden menu bar shows its menus only while one of them is open.
    let show_menus = crate::ui::chrome::toolbars::menubar_visible(st) || open.is_some();

    rsx! {
        div { class: "menubar", onclick: move |e| e.stop_propagation(),
            div { class: "brand", img { src: "{icon}", alt: "" } "Folitecta" }
            if !show_menus {
                button { class: "ibtn", title: "{t(lang, \"menu.show\")}",
                    onclick: move |_| { st.set_open_menu(Some("file".into())); st.set_open_dropdown(None); },
                    Icon { name: "list" }
                }
            }
            for (id, label) in MENUS.iter().filter(|_| show_menus) {
                {
                    let id_s = id.to_string();
                    let id_toggle = id_s.clone();
                    let id_hover = id_s.clone();
                    let is_open = open.as_deref() == Some(*id);
                    let title = ui_label(lang, "menu", id, label);
                    rsx! {
                        div { class: "menu-root", key: "{id}",
                            button {
                                class: if is_open { "menu-title open" } else { "menu-title" },
                                onclick: move |_| {
                                    let cur = st.open_menu.peek().clone();
                                    st.set_open_menu(if cur.as_deref() == Some(id_toggle.as_str()) { None } else { Some(id_toggle.clone()) });
                                    st.set_open_dropdown(None);
                                },
                                onmouseenter: move |_| {
                                    if st.open_menu.peek().is_some() { st.set_open_menu(Some(id_hover.clone())); }
                                },
                                "{title}"
                            }
                            if is_open {
                                div { class: "menu-dropdown",
                                    for entry in reg.menu_entries(&id_s) {
                                        if let Some(cmd) = entry {
                                            {menu_entry(st, lang, cmd, 0)}
                                        } else {
                                            div { class: "menu-sep" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex:1" }
            button { class: "ibtn", title: "{t(lang, \"palette.placeholder\")} (Ctrl+Shift+P)", onclick: move |_| st.set_palette(Some(String::new())), Icon { name: "palette" } }
            button { class: "ibtn", title: "{t(lang, \"dialog.prefs.theme\")}",
                onclick: move |_| { st.ui_mut().theme = theme.toggled(); st.save_prefs(); },
                Icon { name: theme_icon }
            }
            button { class: "ibtn", title: "{t(lang, \"dialog.prefs.language\")}", style: "font-size:11px;font-weight:700",
                onclick: move |_| { let next = if lang == Lang::En { Lang::It } else { Lang::En }; st.ui_mut().lang = next; st.save_prefs(); },
                "{lang.code().to_uppercase()}"
            }
        }
    }
}
