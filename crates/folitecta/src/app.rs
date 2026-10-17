//! Root component: state, page-image protocol, keyboard shortcuts and layout.

use base64::Engine;
use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::Response;
use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::RenderOpts;
use folitecta_core::marks::pdf_rect_to_display;
use folitecta_core::textops::display_rect_to_pdf;

use crate::commands;
use crate::files;
use crate::state::{AppState, Dialog, Theme, Tool};
use crate::ui;
use crate::{ICON_PNG, SelfTest, StartupFiles};

/// The application icon as a data URL for use inside the page.
pub fn icon_data_url() -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ICON_PNG)
    )
}

/// Parse `/page/{tab}/{page}/{revision}/{scale_milli}.png`.
fn parse_page_path(path: &str) -> Option<(u64, usize, f32)> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() < 5 {
        return None;
    }
    let id: u64 = parts[1].parse().ok()?;
    let page: usize = parts[2].parse().ok()?;
    let scale_m: u32 = parts[4].trim_end_matches(".png").parse().ok()?;
    Some((id, page, (scale_m as f32 / 1000.0).clamp(0.05, 8.0)))
}

/// Move the selected comment by a display-space offset (points).
fn nudge_selected(st: AppState, dx: f32, dy: f32) -> bool {
    let Some((page, index)) = st.read_active(|t| t.selected_annot).flatten() else {
        return false;
    };
    let r = st.with_active(|t| {
        let size = t.page_sizes.get(page).copied()?;
        let annot = t
            .doc
            .annotations(page)
            .ok()?
            .into_iter()
            .find(|a| a.index == index)?;
        let d = pdf_rect_to_display(&size, annot.rect);
        let moved = [d[0] + dx, d[1] + dy, d[2] + dx, d[3] + dy];
        let res = t
            .doc
            .set_annotation_rect(page, index, display_rect_to_pdf(&size, moved));
        t.selected_annot = Some((page, index));
        Some(res)
    });
    match r.flatten() {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => {
            st.with_active(|t| t.selected_annot = Some((page, index)));
            files::set_window_title(st);
        }
        None => return false,
    }
    true
}

const VIEWPORT_SCRIPT: &str = r#"
(function(){
  const send = () => dioxus.send([window.innerWidth, window.innerHeight, window.devicePixelRatio || 1]);
  send();
  let pending = null;
  window.addEventListener('resize', () => {
    if (pending) cancelAnimationFrame(pending);
    pending = requestAnimationFrame(send);
  });
  // The text field macros are inserted into (not the search or command boxes).
  document.addEventListener('focusin', e => {
    const t = e.target;
    if (!t || !t.tagName) return;
    const text = t.tagName === 'TEXTAREA' || (t.tagName === 'INPUT' && (t.type === 'text' || t.type === ''));
    if (text && !t.closest('.search-box, .palette, .menubar')) window.__foliField = t;
  });
})();
"#;

#[component]
pub fn App() -> Element {
    let st = AppState::init();
    use_context_provider(|| st);
    let startup = use_context::<StartupFiles>();
    let selftest = use_context::<SelfTest>();

    use_future(move || {
        let cfg = selftest.0.clone();
        async move {
            if let Some(cfg) = cfg {
                crate::selftest::run(st, cfg).await;
            }
        }
    });

    // Open files passed on the command line.
    use_future(move || {
        let files = startup.0.clone();
        async move {
            for f in files {
                files::open_path(st, f, None);
            }
        }
    });

    // Track the window size and pixel ratio (initially and on every resize).
    use_future(move || async move {
        let mut eval = document::eval(VIEWPORT_SCRIPT);
        while let Ok(v) = eval.recv::<(f64, f64, f64)>().await {
            let (w, h, dpr) = v;
            if (w, h) != *st.viewport.peek() {
                st.set_viewport((w, h));
                // Floating menus were positioned for the old size.
                st.close_menus();
            }
            let dpr = dpr.clamp(1.0, 3.0);
            if (dpr - *st.dpr.peek()).abs() > 0.01 {
                st.set_dpr(dpr);
            }
        }
    });

    // Serve page renders at /page/{tab}/{page}/{revision}/{scale_milli}.png[?bg=0][&annots=0]
    let docs = st.docs;
    use_asset_handler("page", move |request, responder| {
        let uri = request.uri();
        let query = uri.query().unwrap_or("");
        let opts = RenderOpts {
            annotations: !query.split('&').any(|kv| kv == "annots=0"),
            transparent: query.split('&').any(|kv| kv == "bg=0"),
        };
        let Some((id, page, scale)) = parse_page_path(uri.path()) else {
            responder.respond(Response::builder().status(404).body(Vec::new()).unwrap());
            return;
        };
        let service = docs.peek().iter().find(|t| t.id == id).map(|t| t.render.clone());
        match service {
            Some(svc) => svc.render_ext(page, scale, opts, move |res| {
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

    // Serve the media of the player at /media/{id}, with byte ranges for seeking.
    use_asset_handler("media", move |request, responder| {
        let range = request
            .headers()
            .get("range")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let (status, headers, body) = commands::media::serve(request.uri().path(), range.as_deref());
        let mut resp = Response::builder().status(status);
        for (k, v) in headers {
            resp = resp.header(k, v);
        }
        responder.respond(resp.body(body).unwrap_or_else(|_| Response::new(Vec::new())));
    });

    // The picture of the picture editor at /imgedit/{generation}.png, encoded on a thread.
    use_asset_handler("imgedit", move |request, responder| {
        let path = request.uri().path().to_string();
        std::thread::spawn(move || {
            let resp = match commands::pictures::preview(&path) {
                Some(png) => Response::builder()
                    .status(200)
                    .header("Content-Type", "image/png")
                    .body(png.to_vec()),
                None => Response::builder().status(404).body(Vec::new()),
            };
            responder.respond(resp.unwrap_or_else(|_| Response::new(Vec::new())));
        });
    });

    // Thumbnails of PDF files in the explorer at /thumb/{hex path}.png, rendered on a thread.
    use_asset_handler("thumb", move |request, responder| {
        let path = request.uri().path().to_string();
        std::thread::spawn(move || {
            let resp = match commands::explorer::thumbnail(&path) {
                Some(png) => Response::builder()
                    .status(200)
                    .header("Content-Type", "image/png")
                    .body(png.to_vec()),
                None => Response::builder().status(404).body(Vec::new()),
            };
            responder.respond(resp.unwrap_or_else(|_| Response::new(Vec::new())));
        });
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
    let (vw, _) = (st.viewport)();
    let compact = if vw < 1100.0 { " compact" } else { "" };
    let touch = if st.ui.read().value("ui.touch") == Some("1") {
        " touch"
    } else {
        ""
    };
    let compact = format!("{compact}{touch}");
    let spellcheck = if commands::option_on(st, "view.spellCheck") {
        "true"
    } else {
        "false"
    };

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
        if commands::interface::auto_scroll_on() {
            match key {
                Key::Escape => {
                    commands::interface::stop_auto_scroll();
                    return;
                }
                Key::ArrowDown | Key::ArrowUp if !mods.ctrl() && !mods.shift() => {
                    evt.prevent_default();
                    commands::interface::auto_scroll_speed(st, key == Key::ArrowDown);
                    return;
                }
                _ => {}
            }
        }
        if key == Key::Escape {
            let menus_open = st.open_menu.peek().is_some()
                || st.open_dropdown.peek().is_some()
                || !st.submenus.peek().is_empty();
            let (has_sel, has_annot) = st
                .read_active(|t| (t.text_sel.is_some(), t.selected_annot.is_some()))
                .unwrap_or((false, false));
            if menus_open {
                st.close_menus();
            } else if st.drawing.peek().is_some() {
                st.set_drawing(None);
            } else if has_sel || has_annot {
                st.with_active(|t| {
                    t.text_sel = None;
                    t.selected_annot = None;
                });
            } else {
                commands::signatures::cancel_pending();
                commands::signs::cancel_pending();
                st.set_tool(Tool::Hand);
                st.set_pending_image(None);
            }
            return;
        }
        if let Key::Character(c) = &key {
            if mods.ctrl() && mods.shift() && c.eq_ignore_ascii_case("p") {
                evt.prevent_default();
                st.set_palette(Some(String::new()));
                return;
            }
            // Copy the text selection before the generic copy command.
            if mods.ctrl() && !mods.shift() && c.eq_ignore_ascii_case("c") {
                let has_text = st
                    .read_active(|t| t.text_sel.map(|s| !s.is_empty()).unwrap_or(false))
                    .unwrap_or(false);
                if has_text {
                    evt.prevent_default();
                    commands::dispatch(st, "tool.selectText.copyAsRich");
                    return;
                }
            }
            // Select all text of the current page with the text tool.
            if mods.ctrl() && !mods.shift() && c.eq_ignore_ascii_case("a") && (st.tool)().is_text() {
                evt.prevent_default();
                st.with_active(|t| {
                    let p = t.page;
                    let n = t.doc.text_layers().get(p).map(|l| l.chars().len()).unwrap_or(0);
                    if n > 0 {
                        t.text_sel = Some(crate::state::TextSel {
                            page: p,
                            anchor: 0,
                            head: n,
                        });
                    }
                });
                return;
            }
        }
        if key == Key::Enter && st.drawing.peek().is_some() {
            evt.prevent_default();
            ui::canvas::finish_drawing(st);
            return;
        }
        if key == Key::Delete || key == Key::Backspace {
            let has_annot = st.read_active(|t| t.selected_annot.is_some()).unwrap_or(false);
            if has_annot {
                evt.prevent_default();
                commands::dispatch(st, "delete");
            }
            return;
        }
        let arrow = match key {
            Key::ArrowLeft => Some((-1.0, 0.0)),
            Key::ArrowRight => Some((1.0, 0.0)),
            Key::ArrowUp => Some((0.0, -1.0)),
            Key::ArrowDown => Some((0.0, 1.0)),
            _ => None,
        };
        if let Some((dx, dy)) = arrow {
            let step = if mods.shift() { 10.0 } else { 1.0 };
            if !mods.ctrl() && st.read_active(|t| t.selected_annot.is_some()).unwrap_or(false) {
                evt.prevent_default();
                nudge_selected(st, dx * step, dy * step);
                return;
            }
        }
        let name: String = match &key {
            Key::Character(c) => c.clone(),
            Key::PageDown => "Page Down".into(),
            Key::PageUp => "Page Up".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::F11 => "F11".into(),
            Key::F1 => "F1".into(),
            Key::F3 => "F3".into(),
            Key::F4 => "F4".into(),
            Key::F5 => "F5".into(),
            Key::F6 => "F6".into(),
            Key::F8 => "F8".into(),
            Key::F9 => "F9".into(),
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
        } else if name == "F3" {
            evt.prevent_default();
            ui::panes::step_search(st, !mods.shift());
        }
    };

    rsx! {
        div {
            class: if fullscreen { "app fullscreen{compact}" } else { "app{compact}" },
            "data-theme": "{theme}",
            "spellcheck": "{spellcheck}",
            tabindex: "0",
            autofocus: true,
            onkeydown: on_key,
            onclick: move |_| st.close_menus(),
            ui::menubar::Menubar {}
            if ribbon {
                ui::ribbon::Ribbon {}
            } else {
                ui::toolbars::ClassicToolbars {}
            }
            div { class: if show_left { "main" } else { "main no-left" },
                ui::panes::PaneStrip {}
                ui::panes::PanePanel {}
                ui::canvas::CanvasArea {}
                ui::media::MediaPlayer {}
                if commands::calculator::CALC_EDIT.read().is_some() {
                    ui::calculator::CalculatorPane {}
                } else if show_right {
                    ui::rightpane::RightPane {}
                }
            }
            ui::statusbar::StatusBar {}
            ui::dialogs::Dialogs {}
            ui::palette::Palette {}
            ui::toasts::Toasts {}
        }
        ui::dialogs::PrintView {}
    }
}

#[cfg(test)]
mod tests {
    use super::parse_page_path;

    #[test]
    fn page_paths_parse() {
        assert_eq!(parse_page_path("/page/3/7/12/1500.png"), Some((3, 7, 1.5)));
        assert_eq!(parse_page_path("/page/3/7"), None);
        assert_eq!(parse_page_path("/page/x/7/1/1000.png"), None);
    }
}
