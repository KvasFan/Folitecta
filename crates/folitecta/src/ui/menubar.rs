//! Classic menu bar built from the command registry.

use dioxus::prelude::*;
use folitecta_core::commands::MENUS;
use folitecta_core::registry;

use crate::app::icon_data_url;
use crate::i18n::{Lang, t, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Theme};
use crate::ui::menu_entry;

#[component]
pub fn Menubar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let open = (st.open_menu)();
    let reg = registry();
    let icon = icon_data_url();
    let theme_dark = st.ui.read().theme == Theme::Dark;
    let theme_icon: &'static str = if theme_dark { "sun" } else { "moon" };

    rsx! {
        div { class: "menubar", onclick: move |e| e.stop_propagation(),
            div { class: "brand", img { src: "{icon}", alt: "" } "Folitecta" }
            for (id, label) in MENUS.iter() {
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
                onclick: move |_| { let next = if theme_dark { Theme::Light } else { Theme::Dark }; st.ui_mut().theme = next; st.save_prefs(); },
                Icon { name: theme_icon }
            }
            button { class: "ibtn", title: "{t(lang, \"dialog.prefs.language\")}", style: "font-size:11px;font-weight:700",
                onclick: move |_| { let next = if lang == Lang::En { Lang::It } else { Lang::En }; st.ui_mut().lang = next; st.save_prefs(); },
                "{lang.code().to_uppercase()}"
            }
        }
    }
}
