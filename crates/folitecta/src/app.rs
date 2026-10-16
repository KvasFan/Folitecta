//! Root component: state, page-image protocol, keyboard shortcuts and layout.

use base64::Engine;
use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::Response;
use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;

use crate::commands;
use crate::files;
use crate::state::{AppState, Dialog, Theme, Tool};
use crate::ui;
use crate::{ICON_PNG, StartupFiles};

/// The application icon as a data URL for use inside the page.
pub fn icon_data_url() -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ICON_PNG)
    )
}

#[component]
pub fn App() -> Element {
    let st = AppState::init();
    use_context_provider(|| st);
    let startup = use_context::<StartupFiles>();

    // Open files passed on the command line and learn the device pixel ratio.
    use_future(move || {
        let files = startup.0.clone();
        async move {
            for f in files {
                files::open_path(st, f, None);
            }
            if let Ok(v) = document::eval("return window.devicePixelRatio || 1;").await {
                if let Some(d) = v.as_f64() {
                    st.set_dpr(d.clamp(1.0, 3.0));
                }
            }
        }
    });

    // Serve page renders at /page/{tab}/{page}/{revision}/{scale_milli}.png
    let docs = st.docs;
    use_asset_handler("page", move |request, responder| {
        let path = request.uri().path().to_string();
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let parsed = (|| -> Option<(u64, usize, f32)> {
            if parts.len() < 5 {
                return None;
            }
            let id: u64 = parts[1].parse().ok()?;
            let page: usize = parts[2].parse().ok()?;
            let scale_m: u32 = parts[4].trim_end_matches(".png").parse().ok()?;
            Some((id, page, (scale_m as f32 / 1000.0).clamp(0.05, 8.0)))
        })();
        let Some((id, page, scale)) = parsed else {
            responder.respond(Response::builder().status(404).body(Vec::new()).unwrap());
            return;
        };
        let service = docs.peek().iter().find(|t| t.id == id).map(|t| t.render.clone());
        match service {
            Some(svc) => svc.render(page, scale, move |res| {
                let resp = match res {
                    Ok(r) => Response::builder()
                        .status(200)
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=31536000, immutable")
                        .body(r.png)
                        .unwrap(),
                    Err(e) => Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(e.to_string().into_bytes())
                        .unwrap(),
                };
                responder.respond(resp);
            }),
            None => responder.respond(Response::builder().status(404).body(Vec::new()).unwrap()),
        }
    });

    let theme = if st.ui.read().theme == Theme::Dark {
        "dark"
    } else {
        "light"
    };
    let ribbon = st.ui.read().ribbon;
    let fullscreen = (st.fullscreen)();
    let show_left = st.ui.read().show_left_pane;
    let show_right = st.ui.read().show_right_pane;

    let on_key = move |evt: Event<KeyboardData>| {
        let key = evt.key();
        let mods = evt.modifiers();
        if !matches!(*st.dialog.read(), Dialog::None) {
            if key == Key::Escape {
                st.set_dialog(Dialog::None);
            }
            return;
        }
        if st.palette.read().is_some() {
            if key == Key::Escape {
                st.set_palette(None);
            }
            return;
        }
        if key == Key::Escape {
            st.close_menus();
            st.set_tool(Tool::Hand);
            return;
        }
        if let Key::Character(c) = &key {
            if mods.ctrl() && mods.shift() && c.eq_ignore_ascii_case("p") {
                evt.prevent_default();
                st.set_palette(Some(String::new()));
                return;
            }
        }
        if key == Key::Delete {
            let has_annot = st.read_active(|t| t.selected_annot.is_some()).unwrap_or(false);
            if has_annot {
                commands::dispatch(st, "delete");
            }
            return;
        }
        let name: String = match &key {
            Key::Character(c) => c.clone(),
            Key::PageDown => "Page Down".into(),
            Key::PageUp => "Page Up".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::F11 => "F11".into(),
            Key::F1 => "F1".into(),
            Key::ArrowLeft => "Left".into(),
            Key::ArrowRight => "Right".into(),
            _ => return,
        };
        let shift = if name == "+" || name == "-" || name == "=" {
            false
        } else {
            mods.shift()
        };
        if let Some(id) = commands::shortcut_command(&name, mods.ctrl(), shift, mods.alt()) {
            evt.prevent_default();
            commands::dispatch(st, id);
        }
    };

    rsx! {
        div {
            class: if fullscreen { "app fullscreen" } else { "app" },
            "data-theme": "{theme}",
            tabindex: "0",
            autofocus: true,
            onkeydown: on_key,
            onclick: move |_| st.close_menus(),
            ui::menubar::Menubar {}
            if ribbon {
                ui::ribbon::Ribbon {}
            } else {
                ui::ribbon::ClassicToolbar {}
            }
            div { class: if show_left { "main" } else { "main no-left" },
                ui::panes::PaneStrip {}
                ui::panes::PanePanel {}
                ui::canvas::CanvasArea {}
                if show_right {
                    ui::rightpane::RightPane {}
                }
            }
            ui::statusbar::StatusBar {}
            ui::dialogs::Dialogs {}
            ui::palette::Palette {}
            ui::toasts::Toasts {}
        }
    }
}
