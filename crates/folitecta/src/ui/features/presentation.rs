//! Presentation (FOL-L001): the pages of the document one at a time on the
//! whole screen, each coming in with the transition the page sets (fade,
//! wipe, split, box, push, cover...) for its time, and going on by itself
//! after the seconds the page shows. Arrows, Space, Page Up and Down, Home
//! and End move; a click goes on; Escape ends.

use dioxus::prelude::*;

use crate::state::AppState;

/// The presentation under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub tab: u64,
    pub page: usize,
    /// Moves made so far: a new one plays the transition again.
    pub step: u64,
    /// The window was full screen before.
    pub was_full: bool,
}

pub static SHOWING: GlobalSignal<Option<Show>> = Signal::global(|| None);

/// Start presenting the active document from the page shown.
pub fn start(st: AppState) {
    let Some((tab, page)) = st.read_active(|t| (t.id, t.page)) else {
        return;
    };
    let was_full = *st.fullscreen.peek();
    *SHOWING.write() = Some(Show {
        tab,
        page,
        step: 0,
        was_full,
    });
    if !was_full {
        st.set_fullscreen(true);
        dioxus::desktop::window().set_fullscreen(true);
    }
    auto_advance(st);
}

/// End the presentation on the page it showed.
pub fn stop(st: AppState) {
    let Some(show) = SHOWING.write().take() else {
        return;
    };
    if !show.was_full {
        st.set_fullscreen(false);
        dioxus::desktop::window().set_fullscreen(false);
    }
    // Once the window has its size again, which moves the view.
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if st.read_active(|t| t.id) == Some(show.tab) {
            st.go_to_page(show.page);
        }
    });
}

/// Show page `page` (within the document), with its transition.
fn show_page(st: AppState, page: usize) {
    let count = st.page_count();
    if count == 0 {
        return;
    }
    let page = page.min(count - 1);
    {
        let mut w = SHOWING.write();
        let Some(show) = w.as_mut() else { return };
        if show.page == page {
            return;
        }
        show.page = page;
        show.step += 1;
    }
    auto_advance(st);
}

/// Move by `by` pages.
fn step(st: AppState, by: isize) {
    let Some(page) = SHOWING.peek().as_ref().map(|s| s.page) else {
        return;
    };
    show_page(st, page.saturating_add_signed(by));
}

/// After the seconds the page shown sets, the next page comes by itself
/// (unless something else moved in between).
fn auto_advance(st: AppState) {
    let Some((page, at)) = SHOWING.peek().as_ref().map(|s| (s.page, s.step)) else {
        return;
    };
    let Some(seconds) = st.read_active(|t| t.doc.page_display_time(page)).flatten() else {
        return;
    };
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs_f32(seconds.clamp(0.1, 3600.0))).await;
        if SHOWING.peek().as_ref().map(|s| s.step) == Some(at) {
            step(st, 1);
        }
    });
}

/// The animation of a transition style of PDF.
fn animation(style: &str) -> &'static str {
    match style {
        "Dissolve" | "Fade" | "Glitter" => "present-fade",
        "Wipe" => "present-wipe",
        "Split" => "present-split",
        "Blinds" => "present-blinds",
        "Box" => "present-box",
        "Push" | "Cover" | "Fly" => "present-push",
        "Uncover" => "present-uncover",
        _ => "",
    }
}

#[component]
pub fn Presentation() -> Element {
    let st = use_context::<AppState>();
    let Some(show) = SHOWING.read().clone() else {
        return rsx! {};
    };
    let Some((tab, rev, size, transition)) = st
        .read_active(|t| {
            (
                t.id,
                t.page_shown.get(show.page).copied().unwrap_or_default(),
                t.page_sizes.get(show.page).copied(),
                t.doc.page_transition(show.page),
            )
        })
        .filter(|(tab, ..)| *tab == show.tab)
    else {
        // The document went away.
        *SHOWING.write() = None;
        return rsx! {};
    };
    let (vw, vh) = (st.viewport)();
    let dpr = (st.dpr)().max(1.0);
    let fit = size
        .map(|s| (vw / s.width as f64).min(vh / s.height as f64))
        .unwrap_or(1.0);
    let scale = ((fit * dpr * 1000.0).round() as u32).clamp(50, 8000);
    let src = format!("/page/{tab}/{}/{rev}/{scale}.png", show.page);
    // The first page comes in without a transition.
    let (class, seconds) = match transition {
        Some((style, seconds)) if show.step > 0 => (animation(&style), seconds.max(0.0)),
        _ => ("", 0.0),
    };
    rsx! {
        div { class: "presentation", tabindex: "0", autofocus: true,
            "data-page": "{show.page}",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onclick: move |_| step(st, 1),
            onkeydown: move |e| {
                e.stop_propagation();
                match e.key() {
                    Key::Escape => stop(st),
                    Key::ArrowRight | Key::ArrowDown | Key::PageDown | Key::Enter => step(st, 1),
                    Key::ArrowLeft | Key::ArrowUp | Key::PageUp | Key::Backspace => step(st, -1),
                    Key::Home => show_page(st, 0),
                    Key::End => show_page(st, usize::MAX),
                    Key::Character(c) if c == " " => step(st, 1),
                    _ => {}
                }
            },
            img { key: "{show.step}", class: "presentation-page {class}", src: "{src}", alt: "",
                style: "animation-duration: {seconds}s" }
        }
    }
}
