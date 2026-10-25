//! Root component: state, page-image protocol, keyboard shortcuts and layout.

use base64::Engine;
use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::Response;
use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::{Lane, RenderAnswer, RenderOpts, RenderRequest};

use crate::commands;
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, Dialog, Tool};
use crate::ui;
use crate::{ICON_PNG, SelfTest, StartupFiles};

/// The application icon as a data URL for use inside the page.
pub fn icon_data_url() -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ICON_PNG)
    )
}

/// A page picture asked for: `/page/{tab}/{page}/{revision}/{scale_milli}.png`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PagePath {
    tab: u64,
    page: usize,
    rev: u64,
    scale: f32,
}

fn parse_page_path(path: &str) -> Option<PagePath> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() < 5 {
        return None;
    }
    let tab: u64 = parts[1].parse().ok()?;
    let page: usize = parts[2].parse().ok()?;
    let rev: u64 = parts[3].parse().ok()?;
    let scale_m: u32 = parts[4].trim_end_matches(".png").parse().ok()?;
    Some(PagePath {
        tab,
        page,
        rev,
        scale: (scale_m as f32 / 1000.0).clamp(0.05, 8.0),
    })
}

/// What the page handler does with the address of a page picture (`path`
/// and `query`): the request for the render threads of the tab it names,
/// with what `find` gives for that tab and page (its render threads, say,
/// and the revision of the page the tab shows: `None` when the tab is
/// closed), or the answer without a picture (status and text). Only the
/// revision the tab shows is drawn; an address of another one gets 409.
fn page_request<T>(
    path: &str,
    query: &str,
    find: impl FnOnce(u64, usize) -> Option<(T, u64)>,
) -> Result<(T, RenderRequest), (u16, &'static str)> {
    let has = |kv: &str| query.split('&').any(|q| q == kv);
    let opts = RenderOpts {
        annotations: !has("annots=0"),
        transparent: has("bg=0"),
    };
    // The pages of the views say so; their pictures are dropped once the
    // views no longer hold them.
    let lane = if has("lane=view") {
        Lane::View
    } else if has("lane=thumb") {
        Lane::Thumb
    } else if has("lane=preview") {
        Lane::Preview
    } else {
        Lane::Other
    };
    let path = parse_page_path(path).ok_or((404, "no such page picture"))?;
    let (found, shown) = find(path.tab, path.page).ok_or((404, "the document is closed"))?;
    if path.rev != shown {
        return Err((409, "the document changed"));
    }
    // A tile of the picture: `tile=column,row`.
    let tile = query
        .split('&')
        .find_map(|q| q.strip_prefix("tile="))
        .and_then(|t| t.split_once(','))
        .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)));
    let req = RenderRequest {
        page: path.page,
        scale: path.scale,
        opts,
        lane,
        rev: Some(path.rev),
        tile,
    };
    Ok((found, req))
}

/// An answer without a picture: never kept by the page, so the same address
/// asked again is asked again.
fn no_picture(status: u16, text: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .header("Cache-Control", "no-store")
        .body(text.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Move the chosen comments (the selected one, its group and those added
/// with Shift) by a display-space offset (points).
fn nudge_selected(st: AppState, dx: f32, dy: f32) -> bool {
    let Some((page, chosen)) = st.read_active(|t| t.chosen_annots()).flatten() else {
        return false;
    };
    match st.with_active(|t| t.doc.shift_annotations(page, &chosen, dx, dy)) {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => {
            st.with_active(|t| t.set_chosen(page, &chosen));
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
  // Once per frame, or after a short wait when no frame comes (a window
  // covered by others or minimized is not drawn).
  let pending = false;
  window.addEventListener('resize', () => {
    if (pending) return;
    pending = true;
    const go = () => { if (pending) { pending = false; send(); } };
    requestAnimationFrame(go);
    setTimeout(go, 100);
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

/// Two fixes to how Dioxus talks to the page.
///
/// Dioxus applies each batch of changes to the page in the next animation
/// frame and runs nothing else (no task, no timer, no event) until the page
/// confirms it. No frame comes while the window is minimized or covered by
/// other windows (the compositor stops drawing it, as on Linux desktops), and
/// the whole program stopped until it was drawn again. The batch is now
/// applied at once while the page is hidden, and after a short wait when no
/// frame comes; otherwise in the next frame, as before. Exactly one
/// confirmation is sent per batch.
///
/// The result of a script run from the program was lost (the error "eval has
/// already ran") when the page collected the script's channel before the
/// program read the result: the page tells the program to forget a channel
/// once it is garbage. The channels of the last scripts are kept a while.
///
/// The events the program listens to on some element go to one listener at
/// the root of the page, which sends every such event to the program and
/// waits for its answer, even when no element on the event's way listens to
/// it; and a listener goes on counting after its element is removed, so
/// once any element had listened to mouse moves, every mouse move of the
/// window waited for the program. Mouse moves, pointer moves and wheel
/// events now reach the program only when an element on their way listens
/// to them (the elements are marked when their listeners are made).
const PAGE_FIXES_SCRIPT: &str = r#"
(function(){
  const it = window.interpreter;
  if (it && !it.__foliFiltered && it.root && ['createListener', 'removeListener'].every(f => typeof it[f] === 'function') && typeof it.handler === 'function') {
    it.__foliFiltered = true;
    const FILTERED = new Set(['mousemove', 'mouseover', 'mouseout', 'pointermove', 'pointerover', 'pointerout', 'wheel', 'touchmove', 'drag', 'dragover']);
    // Kinds already listened to before this point: their elements are not
    // marked, so they keep reaching the program as before.
    const before = new Set(Object.keys(it.global || {}));
    const marks = new WeakMap();
    const mark = (el, name, by) => { if (!el || !FILTERED.has(name)) return; const m = marks.get(el) || {}; m[name] = Math.max(0, (m[name] || 0) + by); marks.set(el, m); };
    const create = it.createListener, remove = it.removeListener;
    it.createListener = function (name, el, bubbles) { if (bubbles) mark(el, name, 1); return create.call(this, name, el, bubbles); };
    it.removeListener = function (el, name, bubbles) { if (bubbles) mark(el, name, -1); return remove.call(this, el, name, bubbles); };
    const listened = e => {
      for (let n = e.target; n; n = n.parentNode) {
        const m = marks.get(n);
        if (m && m[e.type] > 0) return true;
        if (n === it.root) break;
      }
      return false;
    };
    const original = it.handler;
    const filtered = function (e) {
      if (FILTERED.has(e.type) && !before.has(e.type) && !listened(e)) return;
      return original.call(this, e);
    };
    it.handler = filtered;
  }
  if (it && !it.__foliEdits && ['rafEdits', 'enqueueBytes', 'flushQueuedBytes', 'markEditsFinished'].every(f => typeof it[f] === 'function')) {
    it.__foliEdits = true;
    it.rafEdits = function (bytes) {
      this.enqueueBytes(bytes);
      let done = false;
      const apply = () => { if (!done) { done = true; this.flushQueuedBytes(); this.markEditsFinished(); } };
      if (this.headless || document.visibilityState === 'hidden') { apply(); return; }
      const late = setTimeout(apply, 100);
      requestAnimationFrame(() => { clearTimeout(late); apply(); });
    };
  }
  const create = window.createQuery;
  if (typeof create === 'function' && !create.__foliKept) {
    const recent = [];
    const kept = function (id) {
      const query = create(id);
      recent.push(query);
      if (recent.length > 200) recent.shift();
      return query;
    };
    kept.__foliKept = true;
    window.createQuery = kept;
  }
  return true;
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

    // Hardware acceleration of the web view.
    use_hook(|| crate::gpu::apply(&dioxus::desktop::window()));

    // Copies of the documents with unsaved changes, and the ones left behind.
    use_hook(|| crate::recovery::start(st));
    // Decrypted files left by windows that ended without Exit.
    use_hook(crate::private_files::sweep);

    // Open files passed on the command line.
    use_future(move || {
        let files = startup.0.clone();
        async move {
            for f in files {
                files::open_path(st, f, None);
            }
        }
    });

    // Scrolling and Ctrl+wheel zoom of the page views, handled in the page.
    use_future(move || async move {
        ui::canvas::follow_canvas_events(st).await;
    });

    // The ends of what is read aloud.
    use_hook(move || crate::commands::read_aloud::follow(st));

    // Track the window size and pixel ratio (initially and on every resize).
    use_future(move || async move {
        // Changes reach the page also when no frame is drawn; script results are kept.
        let _ = document::eval(PAGE_FIXES_SCRIPT).await;
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

    // Serve page renders at /page/{tab}/{page}/{revision}/{scale_milli}.png[?bg=0][&annots=0][&lane=view|preview|thumb][&tile=column,row].
    // A picture is only given for the revision the tab shows (an address of
    // another revision gets 409, never kept), drawn from the document of that
    // revision or refused when it changed meanwhile.
    let docs = st.docs;
    use_asset_handler("page", move |request, responder| {
        crate::perf::PAGE_REQUESTS.bump();
        let asked = std::time::Instant::now();
        let uri = request.uri();
        let asked_for = page_request(uri.path(), uri.query().unwrap_or(""), |tab, page| {
            docs.peek()
                .iter()
                .find(|t| t.id == tab)
                .map(|t| (t.render.clone(), t.page_rev(page)))
        });
        let (service, req) = match asked_for {
            Ok(found) => found,
            Err((status, text)) => {
                crate::perf::PAGE_OTHER.bump();
                responder.respond(no_picture(status, text));
                return;
            }
        };
        service.request(req, move |answer| {
            crate::perf::PAGE_WAIT_US.add(asked.elapsed().as_micros() as u64);
            let resp = match answer {
                RenderAnswer::Picture(r) => {
                    crate::perf::PAGE_PICTURES.bump();
                    crate::perf::PAGE_BYTES.add(r.png.len() as u64);
                    Response::builder()
                        .status(200)
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=31536000, immutable")
                        .body(r.png.clone())
                        .unwrap_or_else(|_| Response::new(Vec::new()))
                }
                RenderAnswer::Stale => {
                    crate::perf::PAGE_OTHER.bump();
                    no_picture(409, "no longer shown")
                }
                RenderAnswer::Failed(e) => {
                    crate::perf::PAGE_OTHER.bump();
                    no_picture(500, &e)
                }
            };
            responder.respond(resp);
        });
    });

    // The sheets of the print window's preview at /sheet/p/{generation}.png and
    // the sheets laid out for the system dialog at /sheet/s/{generation}/{doc}/{sheet}/{scale}.png.
    use_asset_handler("sheet", move |request, responder| {
        let path = request.uri().path().to_string();
        commands::print::serve(&path, move |status, body| {
            let resp = Response::builder()
                .status(status)
                .header(
                    "Content-Type",
                    if status == 200 { "image/png" } else { "text/plain" },
                )
                .header("Cache-Control", "no-store")
                .body(body);
            responder.respond(resp.unwrap_or_else(|_| Response::new(Vec::new())));
        });
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

    let theme = st.ui.read().theme.attr();
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
    // Widths of the side panes chosen by dragging their edges.
    use ui::chrome::layout::{Axis, LEFT_W, RIGHT_W, Splitter, size};
    let (def_left, def_right) = ui::chrome::layout::default_widths(vw);
    let left_w = size(st, LEFT_W);
    let right_w = size(st, RIGHT_W);
    // Always both widths (the defaults as well): an empty style attribute
    // would not take away the widths dragged before.
    let main_style = format!(
        "--pane-w:{}px;--right-w:{}px",
        left_w.unwrap_or(def_left),
        right_w.unwrap_or(def_right)
    );
    let calc_open = commands::calculator::CALC_EDIT.read().is_some();
    let resize_title = t(st.lang(), "layout.resize");
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
        // The edit text tool: Delete, Backspace and typing change the page text.
        if !mods.ctrl() && !mods.alt() && !mods.meta() && commands::textedit::key(st, &key) {
            evt.prevent_default();
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
                    let n = t.doc.page_text_layer(p).map(|l| l.chars().len()).unwrap_or(0);
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
            ui::chrome::menubar::Menubar {}
            if ribbon {
                ui::chrome::ribbon::Ribbon {}
            } else {
                ui::chrome::toolbars::ClassicToolbars {}
            }
            div { class: if show_left { "main" } else { "main no-left" }, style: "{main_style}",
                ui::panes::PaneStrip {}
                ui::panes::PanePanel {}
                ui::canvas::CanvasArea {}
                ui::features::media::MediaPlayer {}
                if calc_open {
                    ui::features::calculator::CalculatorPane {}
                } else if show_right {
                    ui::panes::right::RightPane {}
                }
                if show_left {
                    Splitter { name: LEFT_W, axis: Axis::X, class: "left-edge", title: resize_title.clone(),
                        base: left_w.unwrap_or(def_left), per_px: 1.0, min: 170.0, max: (vw * 0.6).min(760.0) }
                }
                if show_right && !calc_open {
                    Splitter { name: RIGHT_W, axis: Axis::X, class: "right-edge", title: resize_title.clone(),
                        base: right_w.unwrap_or(def_right), per_px: -1.0, min: 200.0, max: (vw * 0.5).min(700.0) }
                }
            }
            ui::chrome::statusbar::StatusBar {}
            ui::dialogs::Dialogs {}
            ui::chrome::palette::Palette {}
            ui::chrome::toasts::Toasts {}
            ui::chrome::layout::DragShield {}
        }
        ui::dialogs::PrintView {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Lane, page_request, parse_page_path};

    /// The decision of the page handler for an address, the tab 3 showing
    /// revision 12 of every page.
    fn decide(path: &str, query: &str) -> Result<super::RenderRequest, (u16, &'static str)> {
        page_request(path, query, |tab, _| (tab == 3).then_some(((), 12))).map(|(_, r)| r)
    }

    #[test]
    fn only_the_revision_a_tab_shows_is_drawn() {
        let r = decide("/page/3/7/12/1500.png", "lane=view&tile=2,5").unwrap();
        assert_eq!(
            (r.page, r.rev, r.tile, r.lane),
            (7, Some(12), Some((2, 5)), Lane::View)
        );
        assert!((r.scale - 1.5).abs() < 1e-6);
        assert!(r.opts.annotations && !r.opts.transparent);
        let r = decide("/page/3/0/12/800.png", "bg=0&annots=0&lane=thumb&retry=1").unwrap();
        assert_eq!(r.lane, Lane::Thumb);
        assert!(!r.opts.annotations && r.opts.transparent);
        assert_eq!(decide("/page/3/0/12/800.png", "").unwrap().lane, Lane::Other);
        // Another revision of the page (before or after the one shown): refused, 409.
        assert_eq!(decide("/page/3/7/11/1500.png", "lane=view").unwrap_err().0, 409);
        assert_eq!(decide("/page/3/7/13/1500.png", "").unwrap_err().0, 409);
        // A tab closed, an address that is not one: 404.
        assert_eq!(decide("/page/4/7/12/1500.png", "").unwrap_err().0, 404);
        assert_eq!(decide("/page/3/7", "").unwrap_err().0, 404);
    }

    #[test]
    fn page_paths_parse() {
        assert_eq!(
            parse_page_path("/page/3/7/12/1500.png"),
            Some(super::PagePath {
                tab: 3,
                page: 7,
                rev: 12,
                scale: 1.5
            })
        );
        assert_eq!(parse_page_path("/page/3/7"), None);
        assert_eq!(parse_page_path("/page/x/7/1/1000.png"), None);
    }
}
