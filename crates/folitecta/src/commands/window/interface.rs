//! Interface commands: tab titles, toolbars, touch mode, automatic scrolling,
//! the pan and zoom window, window layout and launching the document elsewhere.

use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::commands::{handlers, no_doc, option_on};
use crate::i18n::t;
use crate::state::{AppState, Dialog, Pane, ToastKind};
use crate::ui::chrome::toolbars::{
    DEFAULT_TOOLBARS, TOOLBARS, menubar_visible, set_visible_toolbars, visible_toolbars,
};

/// Automatic scrolling is running.
static AUTO_SCROLL: AtomicBool = AtomicBool::new(false);

pub fn auto_scroll_on() -> bool {
    AUTO_SCROLL.load(Ordering::Relaxed)
}

/// Start or stop scrolling the pages automatically.
pub fn toggle_auto_scroll(st: AppState) {
    if !auto_scroll_on() && no_doc(st) {
        return;
    }
    let speed: f64 = st
        .ui
        .peek()
        .value("autoscroll.speed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.5);
    let js = format!(
        "if (window.__foliAuto) {{ clearInterval(window.__foliAuto.t); window.__foliAuto = null; return false; }}
         const s = {{ speed: {speed}, t: 0 }};
         s.t = setInterval(() => {{ const c = document.getElementById('canvas'); if (!c) return; c.scrollTop += s.speed;
           if (c.scrollTop + c.clientHeight >= c.scrollHeight - 1) {{ clearInterval(s.t); window.__foliAuto = null; }} }}, 30);
         window.__foliAuto = s; return true;"
    );
    spawn(async move {
        let on = document::eval(&js)
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        AUTO_SCROLL.store(on, Ordering::Relaxed);
        if on {
            st.toast(ToastKind::Info, t(st.lang(), "toast.auto_scroll"), "");
        }
    });
}

/// Change the automatic scrolling speed (pixels per step).
pub fn auto_scroll_speed(st: AppState, faster: bool) {
    let speed: f64 = st
        .ui
        .peek()
        .value("autoscroll.speed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.5);
    let next = if faster {
        (speed * 1.4).min(40.0)
    } else {
        (speed / 1.4).max(0.3)
    };
    st.set_value("autoscroll.speed", format!("{next:.2}"));
    let js = format!("if (window.__foliAuto) window.__foliAuto.speed = {next};");
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

/// Stop automatic scrolling when it runs.
pub fn stop_auto_scroll() {
    if auto_scroll_on() {
        AUTO_SCROLL.store(false, Ordering::Relaxed);
        spawn(async move {
            let _ = document::eval(
                "if (window.__foliAuto) { clearInterval(window.__foliAuto.t); window.__foliAuto = null; }",
            )
            .await;
        });
    }
}

fn toggle_toolbar(st: AppState, id: &str) {
    let mut v = visible_toolbars(st);
    if let Some(i) = v.iter().position(|x| x == id) {
        v.remove(i);
    } else {
        v.push(id.to_string());
        if st.ui.peek().ribbon {
            // Toolbars belong to the classic interface.
            st.ui_mut().ribbon = false;
            st.save_prefs();
            st.toast(ToastKind::Info, t(st.lang(), "toast.classic_interface"), "");
        }
    }
    set_visible_toolbars(st, &v);
}

fn toggle_all_toolbars(st: AppState) {
    if visible_toolbars(st).is_empty() {
        set_visible_toolbars(
            st,
            &DEFAULT_TOOLBARS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
    } else {
        set_visible_toolbars(st, &[]);
    }
}

fn restore_toolbars(st: AppState) {
    set_visible_toolbars(
        st,
        &DEFAULT_TOOLBARS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    st.set_value("menubar.hidden", "");
}

fn sort_tabs(st: AppState) {
    let active_id = st.read_active(|t| t.id);
    {
        let mut docs = st.docs_mut();
        docs.sort_by_key(|t| t.title().to_lowercase());
    }
    if let Some(id) = active_id {
        let i = st.docs.read().iter().position(|t| t.id == id).unwrap_or(0);
        st.set_active(i);
    }
}

fn reset_main_panes(st: AppState) {
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = Pane::Thumbnails.id();
        ui.show_right_pane = false;
    }
    st.set_value("pane.width", "");
    st.save_prefs();
}

fn reset_doc_panes(st: AppState) {
    st.ui_mut().show_right_pane = false;
    st.with_active(|t| {
        t.selected_annot = None;
        t.flash = None;
    });
    st.save_prefs();
}

fn focus_page_number() {
    spawn(async move {
        let _ = document::eval("const i = document.querySelector('.statusbar input.page-input, .statusbar input'); if (i) { i.focus(); i.select(); }").await;
    });
}

/// Check state of interface commands.
pub fn interface_state(st: AppState, id: &str) -> Option<bool> {
    if let Some(tb) = id.strip_prefix("view.toolbar.") {
        if tb == "menu" {
            return Some(menubar_visible(st));
        }
        if TOOLBARS.iter().any(|(t, _)| *t == tb) {
            return Some(visible_toolbars(st).iter().any(|v| v == tb));
        }
        return None;
    }
    let ui = st.ui.read();
    let title = ui.value("tabs.title").unwrap_or("auto").to_string();
    let ellipsis = ui.value("tabs.ellipsis").unwrap_or("end").to_string();
    let touch = ui.value("ui.touch") == Some("1");
    drop(ui);
    match id {
        "tabTitleStyle.auto" => Some(title == "auto"),
        "tabTitleStyle.documentFilename" => Some(title == "filename"),
        "tabTitleStyle.documentTitle" => Some(title == "title"),
        "tabTitleStyle.endEllipsis" => Some(ellipsis == "end"),
        "tabTitleStyle.midEllipsis" => Some(ellipsis == "mid"),
        "view.touchMouseMode" | "view.switchToTouchUI" => Some(touch),
        "view.switchToMouseUI" => Some(!touch),
        "view.layout.autoScroll" => Some(auto_scroll_on()),
        "tool.panZoom" => Some(option_on(st, "view.panzoom")),
        _ => None,
    }
}

handlers! {
    "tabTitleStyle.auto" => |st| st.set_value("tabs.title", "auto"),
    "tabTitleStyle.documentFilename" => |st| st.set_value("tabs.title", "filename"),
    "tabTitleStyle.documentTitle" => |st| st.set_value("tabs.title", "title"),
    "tabTitleStyle.endEllipsis" => |st| st.set_value("tabs.ellipsis", "end"),
    "tabTitleStyle.midEllipsis" => |st| st.set_value("tabs.ellipsis", "mid"),
    "view.layout.autoScroll" => toggle_auto_scroll,
    "view.pageNumber" => |_| focus_page_number(),
    "view.toolbar.file" => |st| toggle_toolbar(st, "file"),
    "view.toolbar.standard" => |st| toggle_toolbar(st, "standard"),
    "view.toolbar.navigation" => |st| toggle_toolbar(st, "navigation"),
    "view.toolbar.zoom" => |st| toggle_toolbar(st, "zoom"),
    "view.toolbar.pageZoom" => |st| toggle_toolbar(st, "pageZoom"),
    "view.toolbar.pageLayout" => |st| toggle_toolbar(st, "pageLayout"),
    "view.toolbar.rotateview" => |st| toggle_toolbar(st, "rotateview"),
    "view.toolbar.properties" => |st| toggle_toolbar(st, "properties"),
    "view.toolbar.drawing" => |st| toggle_toolbar(st, "drawing"),
    "view.toolbar.measuring" => |st| toggle_toolbar(st, "measuring"),
    "view.toolbar.form" => |st| toggle_toolbar(st, "form"),
    "view.toolbar.contentEditing" => |st| toggle_toolbar(st, "contentEditing"),
    "view.toolbar.launchApp" => |st| toggle_toolbar(st, "launchApp"),
    "view.toolbar.docOptions" => |st| toggle_toolbar(st, "docOptions"),
    "view.toolbar.view" => |st| toggle_toolbar(st, "view"),
    "view.toolbar.menu" => |st| { let hidden = !menubar_visible(st); st.set_value("menubar.hidden", if hidden { "" } else { "1" }); },
    "view.toolbars" => |st| st.set_dialog(Dialog::Toolbars),
    "view.toolbar.customize" => |st| st.set_dialog(Dialog::Toolbars),
    "view.toggleAllCmdBars" => toggle_all_toolbars,
    "view.restoreCmdBarsVisibility" => restore_toolbars,
    "view.switchToTouchUI" => |st| st.set_value("ui.touch", "1"),
    "view.switchToMouseUI" => |st| st.set_value("ui.touch", ""),
    "view.touchMouseMode" => |st| { let on = st.ui.peek().value("ui.touch") == Some("1"); st.set_value("ui.touch", if on { "" } else { "1" }); },
    "window.sortAllDocTabsAbc" => sort_tabs,
    "window.resetMainPanesLayout" => reset_main_panes,
    "window.resetDocPanesLayout" => reset_doc_panes,
    "zoom.combo" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ZoomLevel) } },
    "tool.panZoom" => |st| { let on = option_on(st, "view.panzoom"); st.set_option("view.panzoom", !on); },
}
