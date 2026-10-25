//! View menu: zoom, navigation, layouts, rotation, panes, themes, rulers and guides.

use crate::commands::{current_zoom, doc_dialog, handlers, no_doc, option_on, set_zoom, toggle_pane};
use crate::files;
use crate::state::{AppState, Dialog, Layout, Pane, Theme, Tool, Zoom};
use dioxus::prelude::*;

fn zoom_step(st: AppState, factor: f32) {
    let next = (current_zoom(st) * factor).clamp(0.1, 8.0);
    set_zoom(st, Zoom::Fixed((next * 100.0).round() / 100.0));
}

fn set_layout(st: AppState, layout: Layout) {
    st.with_active(|t| t.layout = layout);
}

fn set_theme(st: AppState, theme: Theme) {
    st.ui_mut().theme = theme;
    st.save_prefs();
}

fn history_step(st: AppState, back: bool) {
    let target = st
        .with_active(|t| {
            if back && t.history_pos > 0 {
                t.history_pos -= 1;
                t.history.get(t.history_pos).copied()
            } else if !back && t.history_pos + 1 < t.history.len() {
                t.history_pos += 1;
                t.history.get(t.history_pos).copied()
            } else {
                None
            }
        })
        .flatten();
    if let Some(p) = target {
        st.navigate(p, false);
    }
}

fn set_units(st: AppState, units: &str) {
    st.set_value("units", units);
}

fn clear_guides(st: AppState, current_only: bool) {
    st.with_active(|t| {
        if current_only {
            let p = t.page;
            t.guides.retain(|g| g.page != Some(p));
        } else {
            t.guides.clear();
        }
    });
}

fn set_ui(st: AppState, ribbon: bool) {
    st.ui_mut().ribbon = ribbon;
    st.save_prefs();
}

/// The reflow view on or off. The page shown stays: back to the pages, they
/// are scrolled to it once drawn.
fn toggle_reflow(st: AppState) {
    if no_doc(st) {
        return;
    }
    let on = !option_on(st, "view.reflow");
    st.set_option("view.reflow", on);
    if !on {
        let page = st.current_page();
        spawn(async move {
            for wait in [120, 400] {
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                let _ = document::eval(&format!(
                    "document.getElementById('pg-{page}')?.scrollIntoView({{ block: 'start' }});"
                ))
                .await;
            }
        });
    }
}

handlers! {
    "view.reflow" => toggle_reflow,
    "zoomIn" => |st| zoom_step(st, 1.25),
    "zoomOut" => |st| zoom_step(st, 0.8),
    "view.zoom" => |st| doc_dialog(st, Dialog::GoToPage),
    "zoom.to.level" => |st| doc_dialog(st, Dialog::GoToPage),
    "zoom.level.actual" => |st| set_zoom(st, Zoom::Fixed(1.0)),
    "zoom.level.fitPage" => |st| set_zoom(st, Zoom::FitPage),
    "zoom.level.fitHeight" => |st| set_zoom(st, Zoom::FitPage),
    "zoom.level.fitWidth" => |st| set_zoom(st, Zoom::FitWidth),
    "zoom.level.fitVisible" => |st| set_zoom(st, Zoom::FitWidth),
    "zoomSel" => |st| set_zoom(st, Zoom::FitPage),
    "view.goto.firstPage" => |st| st.go_to_page(0),
    "view.goto.lastPage" => |st| { let n = st.page_count(); st.go_to_page(n.saturating_sub(1)); },
    "view.goto.nextPage" => |st| { let p = st.current_page(); st.navigate(p + 1, false); },
    "view.goto.prevPage" => |st| { let p = st.current_page(); st.navigate(p.saturating_sub(1), false); },
    "view.goto.page" => |st| doc_dialog(st, Dialog::GoToPage),
    "view.goto.pagesView" => |st| doc_dialog(st, Dialog::GoToPage),
    "view.goto.prevView" => |st| history_step(st, true),
    "view.goto.nextView" => |st| history_step(st, false),
    "view.goto.nextDocument" => |st| { let n = st.docs.read().len(); if n > 0 { let a = (st.active)(); st.set_active((a + 1) % n); files::set_window_title(st); } },
    "view.goto.prevDocument" => |st| { let n = st.docs.read().len(); if n > 0 { let a = (st.active)(); st.set_active((a + n - 1) % n); files::set_window_title(st); } },
    "view.layout.onePage" => |st| set_layout(st, Layout::Single),
    "view.layout.oneColumn" => |st| set_layout(st, Layout::Continuous),
    "view.layout.twoPages" => |st| set_layout(st, Layout::TwoPages),
    "view.layout.twoColumns" => |st| set_layout(st, Layout::TwoPages),
    "view.layout.standard" => |st| set_layout(st, Layout::Continuous),
    "view.rotate.CW" => |st| { st.with_active(|t| t.view_rotation = (t.view_rotation + 90) % 360); },
    "view.rotate.CCW" => |st| { st.with_active(|t| t.view_rotation = (t.view_rotation + 270) % 360); },
    "view.rotate.reset" => |st| { st.with_active(|t| t.view_rotation = 0); },
    "view.fullScreen" => |st| { let on = !(st.fullscreen)(); st.set_fullscreen(on); dioxus::desktop::window().set_fullscreen(on); },
    "view.fullScreenExit" => |st| { st.set_fullscreen(false); dioxus::desktop::window().set_fullscreen(false); },
    "view.theme.default" => |st| set_theme(st, Theme::Light),
    "view.theme.white" => |st| set_theme(st, Theme::White),
    "view.theme.darkGray" => |st| set_theme(st, Theme::Dark),
    "view.theme.black" => |st| set_theme(st, Theme::Black),
    "view.theme.customize" => |st| st.set_dialog(Dialog::Prefs),
    "view.recovery" => |st| st.set_dialog(Dialog::Recovery),
    "view.pageThumbnails" => |st| toggle_pane(st, Pane::Thumbnails),
    "view.bookmarks" => |st| toggle_pane(st, Pane::Bookmarks),
    "view.comments" => |st| toggle_pane(st, Pane::Comments),
    "view.attachments" => |st| toggle_pane(st, Pane::Attachments),
    "view.layers" => |st| toggle_pane(st, Pane::Layers),
    "view.fields" => |st| toggle_pane(st, Pane::Fields),
    "view.links" => |st| toggle_pane(st, Pane::Links),
    "view.destinations" => |st| toggle_pane(st, Pane::Destinations),
    "view.signatures" => |st| toggle_pane(st, Pane::Signatures),
    "view.search" => |st| toggle_pane(st, Pane::Search),
    "view.properties" => |st| { let v = !st.ui.read().show_right_pane; st.ui_mut().show_right_pane = v; st.save_prefs(); },
    "view.toggleAllCmdPanes" => |st| { let v = !st.ui.read().show_left_pane; st.ui_mut().show_left_pane = v; st.save_prefs(); },
    "view.switchToClassicUI" => |st| set_ui(st, false),
    "view.switchToRibbonUI" => |st| set_ui(st, true),
    "view.ribbonClassicUI" => |st| { let v = !st.ui.read().ribbon; set_ui(st, v); },
    "view.loupe" => |st| { if !no_doc(st) { st.set_tool(Tool::Loupe) } },
    "tool.loupe" => |st| { if !no_doc(st) { st.set_tool(Tool::Loupe) } },
    "rulers.units.points" => |st| set_units(st, "pt"),
    "rulers.units.millimeters" => |st| set_units(st, "mm"),
    "rulers.units.centimeters" => |st| set_units(st, "cm"),
    "rulers.units.inches" => |st| set_units(st, "in"),
    "rulers.units.picas" => |st| set_units(st, "pc"),
    "rulers.addNewGuides" => |st| doc_dialog(st, Dialog::AddGuides),
    "rulers.deleteAllGuides" => |st| clear_guides(st, false),
    "rulers.deleteGuidesOnCurrentPage" => |st| clear_guides(st, true),
}
