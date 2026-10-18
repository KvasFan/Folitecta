//! Sizes the user changes by dragging: the width of the side panes, the rows
//! of the ribbon and how many tools each ribbon group shows.
//!
//! Each size is a saved value. While a drag goes on, the size lives in
//! [`LIVE`] (nothing is written to disk on every move) and it is saved when
//! the button is released. A double click on a handle restores the default.

use dioxus::prelude::*;

use crate::state::AppState;

/// Width of the left pane, in pixels.
pub const LEFT_W: &str = "layout.leftWidth";
/// Width of the right pane, in pixels.
pub const RIGHT_W: &str = "layout.rightWidth";
/// Rows of icons in each column of the ribbon (the tool names hidden).
pub const RIBBON_ROWS: &str = "layout.ribbonRows";
/// Rows of tiles in each column of the ribbon (an icon above its name).
pub const RIBBON_TILE_ROWS: &str = "layout.ribbonTileRows";

/// Saved value of how many tools a ribbon group shows.
pub fn group_key(tab: &str, group: &str) -> String {
    format!("layout.group.{tab}.{group}")
}

/// Direction a handle moves in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
}

/// A drag in progress: the size it changes and how the pointer changes it.
#[derive(Clone, PartialEq, Debug)]
pub struct Drag {
    pub key: String,
    pub axis: Axis,
    /// Pointer position where the drag started, on its axis.
    pub origin: f64,
    /// The size when the drag started.
    pub base: f64,
    /// Change of the size for each pixel the pointer moves.
    pub per_px: f64,
    pub min: f64,
    pub max: f64,
    /// Where the handle is for each size from `min` up, when the sizes are
    /// not evenly spaced (the tools of a ribbon group); empty otherwise.
    pub stops: Vec<f64>,
    /// Where the handle was when the drag started, measured like `stops`.
    pub start: f64,
}

impl Drag {
    /// The size for a pointer at `pos`, whole and within its limits: the one
    /// whose handle is nearest to the pointer when there are stops.
    pub fn value_at(&self, pos: f64) -> f64 {
        if !self.stops.is_empty() {
            let target = self.start + (pos - self.origin);
            let nearest = self
                .stops
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1 - target).abs().total_cmp(&(b.1 - target).abs()))
                .map(|(i, _)| i)
                .unwrap_or(0);
            return (self.min + nearest as f64).clamp(self.min, self.max.max(self.min));
        }
        (self.base + (pos - self.origin) * self.per_px)
            .round()
            .clamp(self.min, self.max.max(self.min))
    }
}

pub static DRAG: GlobalSignal<Option<Drag>> = Signal::global(|| None);
/// The size being dragged (key, value), before it is saved.
pub static LIVE: GlobalSignal<Option<(String, f64)>> = Signal::global(|| None);

/// Current value of a size: the one being dragged, else the saved one.
pub fn size(st: AppState, key: &str) -> Option<f64> {
    if let Some((k, v)) = LIVE.read().as_ref() {
        if k == key {
            return Some(*v);
        }
    }
    st.ui.read().value(key).and_then(|v| v.parse().ok())
}

/// The saved value of the rows of the ribbon, its default and its largest
/// value, for tiles or icons alone.
pub fn ribbon_rows_key(tiles: bool) -> (&'static str, usize, usize) {
    use crate::ui::chrome::ribbon_layout::{DEFAULT_ROWS, DEFAULT_TILE_ROWS, MAX_TILE_ROWS};
    if tiles {
        (RIBBON_TILE_ROWS, DEFAULT_TILE_ROWS, MAX_TILE_ROWS)
    } else {
        (RIBBON_ROWS, DEFAULT_ROWS, 6)
    }
}

/// Rows of tools in a column of the ribbon: tiles, or icons alone.
pub fn ribbon_rows(st: AppState, tiles: bool) -> usize {
    let (key, default, max) = ribbon_rows_key(tiles);
    size(st, key)
        .map(|r| r.clamp(1.0, max as f64) as usize)
        .unwrap_or(default)
}

/// Default width of the left and right panes for a window `vw` pixels wide
/// (the same as the stylesheet's).
pub fn default_widths(vw: f64) -> (f64, f64) {
    if vw < 1100.0 {
        (230.0, 230.0)
    } else {
        ((vw * 0.22).clamp(220.0, 320.0), (vw * 0.20).clamp(220.0, 300.0))
    }
}

fn finish(st: AppState) {
    let live = LIVE.write().take();
    DRAG.write().take();
    if let Some((key, v)) = live {
        st.set_value(&key, format!("{v}"));
    }
}

/// Forget a size: the part goes back to its default (or automatic) size.
pub fn reset(st: AppState, key: &str) {
    if st.ui_mut().values.remove(key).is_some() {
        st.save_prefs();
    }
}

/// A handle that changes a size when dragged.
#[component]
pub fn Splitter(
    #[props(into)] name: String,
    axis: Axis,
    #[props(into)] class: String,
    base: f64,
    per_px: f64,
    min: f64,
    max: f64,
    #[props(into, default)] title: String,
    /// Where the handle is for each size from `min` up (see [`Drag::stops`]).
    #[props(default)]
    stops: Vec<f64>,
    /// Where the handle is now, measured like `stops`.
    #[props(default)]
    start: f64,
) -> Element {
    let st = use_context::<AppState>();
    let dragging = DRAG.read().as_ref().is_some_and(|d| d.key == name);
    let dir = if axis == Axis::X { "v" } else { "h" };
    let classes = format!(
        "splitter {dir} {class}{}",
        if dragging { " dragging" } else { "" }
    );
    let reset_key = name.clone();
    rsx! {
        div { class: "{classes}", title: "{title}", "data-size": "{name}",
            onmousedown: move |e| {
                e.stop_propagation();
                e.prevent_default();
                let c = e.client_coordinates();
                *DRAG.write() = Some(Drag {
                    key: name.clone(),
                    axis,
                    origin: if axis == Axis::X { c.x } else { c.y },
                    base,
                    per_px,
                    min,
                    max,
                    stops: stops.clone(),
                    start,
                });
            },
            ondoubleclick: move |e| {
                e.stop_propagation();
                reset(st, &reset_key);
            },
            onclick: move |e| e.stop_propagation(),
        }
    }
}

/// While a handle is dragged: a layer over the window that follows the
/// pointer, so the drag goes on over every part, and ends it on release.
#[component]
pub fn DragShield() -> Element {
    let st = use_context::<AppState>();
    let Some(drag) = DRAG.read().clone() else {
        return rsx! {};
    };
    let class = if drag.axis == Axis::X {
        "drag-shield v"
    } else {
        "drag-shield h"
    };
    rsx! {
        div { class,
            onmousemove: move |e| {
                // Moves without a button pressed are not part of the drag (the
                // webview makes some up when the page changes under the pointer).
                if e.held_buttons().is_empty() {
                    return;
                }
                let c = e.client_coordinates();
                let v = drag.value_at(if drag.axis == Axis::X { c.x } else { c.y });
                let changed = LIVE.peek().as_ref().map(|(_, x)| *x) != Some(v);
                if changed {
                    *LIVE.write() = Some((drag.key.clone(), v));
                }
            },
            onmouseup: move |_| finish(st),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drags_change_sizes_within_limits() {
        let d = Drag {
            key: LEFT_W.into(),
            axis: Axis::X,
            origin: 100.0,
            base: 260.0,
            per_px: 1.0,
            min: 180.0,
            max: 600.0,
            stops: Vec::new(),
            start: 0.0,
        };
        assert_eq!(d.value_at(150.4), 310.0);
        assert_eq!(d.value_at(-500.0), 180.0);
        assert_eq!(d.value_at(5000.0), 600.0);
        // The right pane grows when its left edge goes left.
        let r = Drag {
            per_px: -1.0,
            ..d.clone()
        };
        assert_eq!(r.value_at(60.0), 300.0);
        // Rows: one more for every row height dragged down.
        let rows = Drag {
            axis: Axis::Y,
            base: 3.0,
            per_px: 1.0 / 21.0,
            min: 1.0,
            max: 6.0,
            ..d
        };
        assert_eq!(rows.value_at(100.0 + 44.0), 5.0);
        assert_eq!(rows.value_at(100.0 + 400.0), 6.0);
        assert_eq!(default_widths(1440.0), (316.8, 288.0));
    }

    #[test]
    fn group_edges_follow_the_pointer() {
        // A group 120 px wide whose edge is at 70, 120, 120, 170 and 205 px
        // for one to five tools: the tools whose edge is nearest the pointer.
        let d = Drag {
            key: "layout.group.home.comment".into(),
            axis: Axis::X,
            origin: 500.0,
            base: 3.0,
            per_px: 0.0,
            min: 1.0,
            max: 5.0,
            stops: vec![70.0, 120.0, 120.0, 170.0, 205.0],
            start: 120.0,
        };
        assert_eq!(d.value_at(500.0), 2.0);
        assert_eq!(d.value_at(552.0), 4.0);
        assert_eq!(d.value_at(590.0), 5.0);
        assert_eq!(d.value_at(5000.0), 5.0);
        assert_eq!(d.value_at(440.0), 1.0);
        assert_eq!(d.value_at(-900.0), 1.0);
    }
}
