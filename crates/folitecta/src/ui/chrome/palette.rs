//! Command palette: fuzzy access to every command.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::i18n::{cmd_label, t};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::status_badge;

#[component]
pub fn Palette() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(query) = (st.palette)() else {
        return rsx! {};
    };
    let mut selected = use_signal(|| 0usize);
    let reg = registry();
    let results: Vec<&'static folitecta_core::Command> = if query.trim().is_empty() {
        let recent_ids = [
            "open",
            "save",
            "docProps",
            "document.rotatePages",
            "document.insertEmptyPages",
            "document.watermarks.add",
            "view.pageThumbnails",
            "help.about",
        ];
        recent_ids.iter().filter_map(|id| reg.get(id)).collect()
    } else {
        reg.search(&query, 40)
    };
    let n = results.len();
    let ids: Vec<String> = results.iter().map(|c| c.id.clone()).collect();

    rsx! {
        div { class: "palette-backdrop", onclick: move |_| st.set_palette(None),
            div { class: "palette", onclick: move |e| e.stop_propagation(),
                input {
                    autofocus: true,
                    placeholder: "{t(lang, \"palette.placeholder\")}",
                    value: "{query}",
                    oninput: move |e| { st.set_palette(Some(e.value())); selected.set(0); },
                    onkeydown: move |e| {
                        e.stop_propagation();
                        match e.key() {
                            Key::Escape => st.set_palette(None),
                            Key::ArrowDown => { if n > 0 { selected.set((selected() + 1) % n); } }
                            Key::ArrowUp => { if n > 0 { selected.set((selected() + n - 1) % n); } }
                            Key::Enter => {
                                if let Some(id) = ids.get(selected()) { let id = id.clone(); st.set_palette(None); dispatch(st, &id); }
                            }
                            _ => {}
                        }
                    },
                }
                div { class: "results",
                    if results.is_empty() {
                        div { class: "pane-empty", "{t(lang, \"palette.empty\")}" }
                    }
                    for (i, cmd) in results.into_iter().enumerate() {
                        {
                            let id = cmd.id.clone();
                            let label = cmd_label(lang, cmd);
                            let path = reg.path_label(&cmd.id);
                            let icon = crate::icons::for_command(&cmd.id);
                            let shortcut = cmd.shortcut.clone().unwrap_or_default();
                            rsx! {
                                div { class: if i == selected() { "result active" } else { "result" }, key: "{cmd.id}",
                                    onmouseenter: move |_| selected.set(i),
                                    onclick: move |_| { st.set_palette(None); dispatch(st, &id); },
                                    Icon { name: icon }
                                    div { class: "grow", span { "{label}" } span { class: "path", "{path}" } }
                                    if !shortcut.is_empty() { span { class: "shortcut", "{shortcut}" } }
                                    {status_badge(&cmd.id)}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
