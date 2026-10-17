//! The split pages editor: cut lines drawn over a page, then the pages cut
//! along them.

use dioxus::prelude::*;

use super::{handlers, no_doc, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// A cut line: horizontal lines are measured from the top of the page,
/// vertical ones from its left (display points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutLine {
    pub horizontal: bool,
    pub pos: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplitEditor {
    pub lines: Vec<CutLine>,
    pub selected: Option<usize>,
    /// Page shown under the lines.
    pub page: usize,
    /// Zoom of the page (None: it fits the visible area).
    pub zoom: Option<f32>,
    /// Size of the visible area (CSS pixels), measured when the editor opens.
    pub stage: (f32, f32),
    /// Line being dragged.
    pub dragging: Option<usize>,
    /// Clearing the lines waits for a confirmation.
    pub confirm_clear: bool,
}

pub static SPLIT: GlobalSignal<SplitEditor> = Signal::global(SplitEditor::default);

/// Units of the editor: name and points per unit.
pub const UNITS: &[(&str, f32)] = &[
    ("mm", 72.0 / 25.4),
    ("cm", 72.0 / 2.54),
    ("in", 72.0),
    ("pt", 1.0),
];

/// Pages cut: every page, odd or even ones, the page shown.
pub const EVERY: &[&str] = &["all", "odd", "even", "current"];

pub const ZOOMS: &[f32] = &[0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

pub fn units(st: AppState) -> (&'static str, f32) {
    let v = st.value("split.units");
    UNITS.iter().copied().find(|(u, _)| *u == v).unwrap_or(UNITS[0])
}

pub fn every(st: AppState) -> &'static str {
    let v = st.value("split.every");
    EVERY.iter().copied().find(|e| *e == v).unwrap_or("all")
}

pub fn line_color(st: AppState) -> String {
    let v = st.value("split.color");
    if v.starts_with('#') && v.len() == 7 {
        v
    } else {
        "#e11d48".into()
    }
}

/// The pages cut for a choice, among `count` pages with `current` shown.
pub fn pages_for(choice: &str, count: usize, current: usize) -> Vec<usize> {
    match choice {
        // Page numbers as people count them: 1, 3, 5… are odd.
        "odd" => (0..count).step_by(2).collect(),
        "even" => (1..count).step_by(2).collect(),
        "current" => vec![current.min(count.saturating_sub(1))],
        _ => (0..count).collect(),
    }
}

/// Lines cutting a page of `size` into `cols` columns and `rows` rows of the same size.
pub fn even_lines(size: (f32, f32), cols: usize, rows: usize) -> Vec<CutLine> {
    let mut out = Vec::new();
    for c in 1..cols.max(1) {
        out.push(CutLine {
            horizontal: false,
            pos: size.0 * c as f32 / cols as f32,
        });
    }
    for r in 1..rows.max(1) {
        out.push(CutLine {
            horizontal: true,
            pos: size.1 * r as f32 / rows as f32,
        });
    }
    out
}

/// Where a new line goes: the middle of the widest free span.
pub fn free_spot(lines: &[CutLine], horizontal: bool, length: f32) -> f32 {
    let mut cuts: Vec<f32> = lines
        .iter()
        .filter(|l| l.horizontal == horizontal)
        .map(|l| l.pos)
        .collect();
    cuts.push(0.0);
    cuts.push(length);
    cuts.sort_by(|a, b| a.total_cmp(b));
    let (a, b) = cuts
        .windows(2)
        .map(|w| (w[0], w[1]))
        .max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
        .unwrap_or((0.0, length));
    (a + b) / 2.0
}

/// How many parts a page of `size` becomes.
pub fn parts(lines: &[CutLine], size: (f32, f32)) -> usize {
    let inside = |horizontal: bool, max: f32| {
        let mut v: Vec<f32> = lines
            .iter()
            .filter(|l| l.horizontal == horizontal && l.pos > 1.0 && l.pos < max - 1.0)
            .map(|l| l.pos)
            .collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        v.len()
    };
    (inside(false, size.0) + 1) * (inside(true, size.1) + 1)
}

/// Display size of a page of the active document.
pub fn page_size(st: AppState, page: usize) -> (f32, f32) {
    st.read_active(|t| t.page_sizes.get(page).map(|s| (s.width, s.height)))
        .flatten()
        .unwrap_or((595.0, 842.0))
}

fn is_open(st: AppState) -> bool {
    *st.dialog.peek() == Dialog::SplitPages
}

/// Open the editor on the current page.
pub fn open(st: AppState) {
    if no_doc(st) || is_open(st) {
        return;
    }
    *SPLIT.write() = SplitEditor {
        page: st.current_page(),
        ..Default::default()
    };
    st.set_dialog(Dialog::SplitPages);
}

fn add(st: AppState, horizontal: bool) {
    open(st);
    if !is_open(st) {
        return;
    }
    let page = SPLIT.peek().page;
    let (w, h) = page_size(st, page);
    let mut s = SPLIT.write();
    let pos = free_spot(&s.lines, horizontal, if horizontal { h } else { w });
    s.lines.push(CutLine { horizontal, pos });
    s.selected = Some(s.lines.len() - 1);
    s.confirm_clear = false;
}

fn remove_selected(st: AppState) {
    open(st);
    let mut s = SPLIT.write();
    match s.selected {
        Some(i) if i < s.lines.len() => {
            s.lines.remove(i);
            s.selected = if s.lines.is_empty() {
                None
            } else {
                Some(i.min(s.lines.len() - 1))
            };
        }
        _ => {
            drop(s);
            st.toast(ToastKind::Info, t(st.lang(), "toast.split_select_line"), "");
        }
    }
}

fn clear(st: AppState) {
    open(st);
    let mut s = SPLIT.write();
    if s.lines.is_empty() {
        return;
    }
    if s.confirm_clear {
        s.lines.clear();
        s.selected = None;
        s.confirm_clear = false;
    } else {
        s.confirm_clear = true;
    }
}

fn edit_line(st: AppState) {
    open(st);
    let target = {
        let mut s = SPLIT.write();
        if s.selected.is_none() && !s.lines.is_empty() {
            s.selected = Some(0);
        }
        s.selected
    };
    match target {
        Some(i) => {
            let js = format!(
                "setTimeout(() => {{ const i = document.getElementById('split-line-{i}'); if (i) {{ i.focus(); i.select(); }} }}, 60);"
            );
            spawn(async move {
                let _ = document::eval(&js).await;
            });
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.split_select_line"), ""),
    }
}

fn cycle_units(st: AppState) {
    open(st);
    let (u, _) = units(st);
    let i = UNITS.iter().position(|(n, _)| *n == u).unwrap_or(0);
    st.set_value("split.units", UNITS[(i + 1) % UNITS.len()].0);
}

fn cycle_every(st: AppState) {
    open(st);
    let e = every(st);
    let i = EVERY.iter().position(|n| *n == e).unwrap_or(0);
    st.set_value("split.every", EVERY[(i + 1) % EVERY.len()]);
}

fn pick_color(st: AppState) {
    open(st);
    spawn(async move {
        let _ =
            document::eval("setTimeout(() => document.getElementById('split-color')?.click(), 60);").await;
    });
}

/// The zoom of the page as shown.
pub fn zoom_now(st: AppState) -> f32 {
    let s = SPLIT.peek();
    s.zoom.unwrap_or_else(|| fit_zoom(page_size(st, s.page), s.stage))
}

/// The zoom that fits a page in the visible area.
pub fn fit_zoom(size: (f32, f32), stage: (f32, f32)) -> f32 {
    if stage.0 < 10.0 || stage.1 < 10.0 || size.0 <= 0.0 || size.1 <= 0.0 {
        return 0.5;
    }
    ((stage.0 - 40.0) / size.0)
        .min((stage.1 - 40.0) / size.1)
        .clamp(0.1, 2.0)
}

fn zoom_by(st: AppState, up: bool) {
    open(st);
    let z = zoom_now(st);
    let next = if up {
        ZOOMS.iter().copied().find(|v| *v > z + 1e-3).unwrap_or(2.0)
    } else {
        ZOOMS
            .iter()
            .rev()
            .copied()
            .find(|v| *v < z - 1e-3)
            .unwrap_or(0.25)
    };
    SPLIT.write().zoom = Some(next);
}

fn zoom_combo(st: AppState) {
    open(st);
    spawn(async move {
        let _ = document::eval("setTimeout(() => document.getElementById('split-zoom')?.focus(), 60);").await;
    });
}

/// Cut the pages along the lines.
pub fn apply(st: AppState) {
    let (lines, page) = {
        let s = SPLIT.peek();
        (s.lines.clone(), s.page)
    };
    if lines.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.split_no_lines"), "");
        return;
    }
    let xs: Vec<f32> = lines.iter().filter(|l| !l.horizontal).map(|l| l.pos).collect();
    let ys: Vec<f32> = lines.iter().filter(|l| l.horizontal).map(|l| l.pos).collect();
    let pages = pages_for(every(st), st.page_count(), page);
    match st.with_active(|t| t.doc.split_pages_at(&pages, &xs, &ys)) {
        Some(Ok(n)) => {
            st.set_dialog(Dialog::None);
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.pages_split", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Check states of the split pages commands.
pub fn split_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "splitPages.showRulers" => Some(option_on(st, "split.rulers")),
        _ => None,
    }
}

handlers! {
    "document.splitPages" => open,
    "splitPages.addHorizontal" => |st| add(st, true),
    "splitPages.addVertical" => |st| add(st, false),
    "splitPages.editLine" => edit_line,
    "splitPages.removeLine.list" => remove_selected,
    "splitPages.removeLine.preview" => remove_selected,
    "splitPages.clear" => clear,
    "splitPages.units" => cycle_units,
    "splitPages.every" => cycle_every,
    "splitPages.colorPicker" => pick_color,
    "splitPages.showRulers" => |st| { open(st); let on = option_on(st, "split.rulers"); st.set_option("split.rulers", !on); },
    "splitPages.zoomIn" => |st| zoom_by(st, true),
    "splitPages.zoomOut" => |st| zoom_by(st, false),
    "splitPages.zoomCombo" => zoom_combo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_pages_and_parts() {
        assert_eq!(pages_for("odd", 5, 0), vec![0, 2, 4]);
        assert_eq!(pages_for("even", 5, 0), vec![1, 3]);
        assert_eq!(pages_for("current", 5, 9), vec![4]);
        assert_eq!(pages_for("all", 3, 1), vec![0, 1, 2]);
        let grid = even_lines((600.0, 800.0), 3, 2);
        assert_eq!(grid.len(), 3);
        assert_eq!(parts(&grid, (600.0, 800.0)), 6);
        assert!((grid[0].pos - 200.0).abs() < 1e-3 && !grid[0].horizontal);
        assert!((grid[2].pos - 400.0).abs() < 1e-3 && grid[2].horizontal);
        // A new line goes in the middle of the widest free span.
        let one = [CutLine {
            horizontal: true,
            pos: 600.0,
        }];
        assert!((free_spot(&one, true, 800.0) - 300.0).abs() < 1e-3);
        assert!((free_spot(&one, false, 600.0) - 300.0).abs() < 1e-3);
        // Lines on the edges cut nothing.
        assert_eq!(
            parts(
                &[CutLine {
                    horizontal: false,
                    pos: 0.5
                }],
                (600.0, 800.0)
            ),
            1
        );
    }
}
