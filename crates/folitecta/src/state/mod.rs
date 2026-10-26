//! Application state shared through the Dioxus context.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use folitecta_core::{CommentInfo, Document, PageSize, RenderService, SearchHit};
use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

pub mod comment_styles;
mod dialog;
mod doc_tab;
mod prefs;
mod view;

pub use dialog::*;
pub use doc_tab::*;
pub use prefs::*;
pub use view::*;

/// A comparison of two open documents.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareState {
    pub old_tab: u64,
    pub new_tab: u64,
    pub old_name: String,
    pub new_name: String,
    pub result: folitecta_core::compare::Comparison,
    /// Current difference.
    pub current: usize,
    /// Differences shown on the pages: `all`, `list` (the current one) or `none`.
    pub filter: String,
    /// Whole text rows are highlighted.
    pub rows: bool,
    /// The protection of the first protected document compared: the report,
    /// which quotes their text, gets it.
    pub protection: Option<folitecta_core::Protection>,
}

/// All reactive state, cheap to copy into closures.
#[derive(Clone, Copy)]
pub struct AppState {
    pub docs: Signal<Vec<DocTab>>,
    pub active: Signal<usize>,
    pub ui: Signal<UiPrefs>,
    pub dialog: Signal<Dialog>,
    pub toasts: Signal<Vec<Toast>>,
    pub palette: Signal<Option<String>>,
    pub tool: Signal<Tool>,
    pub ribbon_tab: Signal<String>,
    pub open_menu: Signal<Option<String>>,
    pub open_dropdown: Signal<Option<String>>,
    pub canvas_size: Signal<(f64, f64)>,
    pub dpr: Signal<f64>,
    pub fullscreen: Signal<bool>,
    pub next_id: Signal<u64>,
    pub closed_recently: Signal<Vec<PathBuf>>,
    /// Selected documents of the history pane.
    pub history_sel: Signal<Vec<PathBuf>>,
    /// Collapsed groups of the history pane.
    pub history_collapsed: Signal<HashSet<String>>,
    /// Selected session and, inside it, file.
    pub session_sel: Signal<Option<(usize, Option<usize>)>>,
    /// The last comparison of two documents.
    pub comparison: Signal<Option<CompareState>>,
    pub busy: Signal<bool>,
    /// Viewport position (CSS px) where the open ribbon/pane dropdown is anchored.
    pub dropdown_pos: Signal<(f64, f64)>,
    /// Open classic-menu submenus by depth: (command id, x, y).
    pub submenus: Signal<Vec<(String, f64, f64)>>,
    /// Size of the window content area in CSS pixels.
    pub viewport: Signal<(f64, f64)>,
    /// Internal comment clipboard.
    pub clip: Signal<Option<Clip>>,
    /// Picture waiting to be placed with the image tool.
    pub pending_image: Signal<Option<Vec<u8>>>,
    /// Loupe position: page, x and y in display points, pointer client x and y.
    pub loupe: Signal<Option<LoupePos>>,
    /// Pointer position over a page for the status bar: (page, x, y) in display points.
    pub pointer: Signal<Option<(usize, f32, f32)>>,
    /// Pages currently laid out for printing.
    pub print_job: Signal<Option<PrintJob>>,
    /// Points placed so far by a multi-click tool: page and display points.
    pub drawing: Signal<Option<Drawing>>,
    /// Copied comment style.
    pub style_clip: Signal<Option<folitecta_core::AnnotStyle>>,
    /// Form field being typed into on the page: page and widget position in `/Annots`.
    pub editing_field: Signal<Option<(usize, usize)>>,
    /// Radio group that new radio buttons join while the radio tool stays active.
    pub radio_group: Signal<Option<String>>,
    /// The pages each view holds with their content (the others are empty
    /// boxes of the same size): document tab, first and last page, for the
    /// focused view (0) and the other view of a split window (1); `None`
    /// until the view has told which pages it shows.
    pub windows: Signal<[Option<(u64, usize, usize)>; 2]>,
    /// Two groups of tabs (Window › New Vertical Tab Group…), if any.
    pub groups: Signal<Option<TabGroups>>,
}

/// The tabs in two groups side by side or one above the other, each group
/// showing one of its documents with its own tab strip. The tab of the
/// group worked in is the active tab; the other group's document is the
/// other view of its split window (`SplitView::source`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TabGroups {
    pub side_by_side: bool,
    /// The tabs of each group, by id.
    pub tabs: [Vec<u64>; 2],
    /// The tab each group shows.
    pub shown: [u64; 2],
    /// The group worked in (new tabs join it).
    pub focus: usize,
}

impl TabGroups {
    /// The group of tab `id`.
    pub fn group_of(&self, id: u64) -> Option<usize> {
        (0..2).find(|&g| self.tabs[g].contains(&id))
    }
}

/// Where state was written while the interface was rendering a component.
/// Such a write marks every reader of that state to render again: a
/// component that writes state it reads renders over and over, and then
/// the effects of every component wait forever (they run only when nothing
/// is left to render). The self-test reports these places.
static RENDER_WRITES: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());

/// Note a write of the state at the caller's place when it happens while a
/// component renders.
#[track_caller]
fn note_write() {
    if dioxus::core::Runtime::try_current().is_some_and(|r| r.vdom_is_rendering()) {
        let at = std::panic::Location::caller();
        if let Ok(mut places) = RENDER_WRITES.lock() {
            places.insert(format!("{}:{}", at.file(), at.line()));
        }
    }
}

/// The places where state was written while a component rendered.
pub fn render_writes() -> Vec<String> {
    RENDER_WRITES
        .lock()
        .map(|p| p.iter().cloned().collect())
        .unwrap_or_default()
}

impl AppState {
    /// Create the state; must be called from a component (uses hooks).
    pub fn init() -> Self {
        AppState {
            docs: use_signal(Vec::new),
            active: use_signal(|| 0),
            ui: use_signal(UiPrefs::load),
            dialog: use_signal(Dialog::default),
            toasts: use_signal(Vec::new),
            palette: use_signal(|| None),
            tool: use_signal(|| Tool::Hand),
            ribbon_tab: use_signal(|| "home".to_string()),
            open_menu: use_signal(|| None),
            open_dropdown: use_signal(|| None),
            canvas_size: use_signal(|| (1000.0, 700.0)),
            dpr: use_signal(|| 1.25),
            fullscreen: use_signal(|| false),
            next_id: use_signal(|| 1),
            closed_recently: use_signal(Vec::new),
            history_sel: use_signal(Vec::new),
            history_collapsed: use_signal(HashSet::new),
            session_sel: use_signal(|| None),
            comparison: use_signal(|| None),
            busy: use_signal(|| false),
            dropdown_pos: use_signal(|| (0.0, 0.0)),
            submenus: use_signal(Vec::new),
            viewport: use_signal(|| (1440.0, 900.0)),
            clip: use_signal(|| None),
            pending_image: use_signal(|| None),
            loupe: use_signal(|| None),
            pointer: use_signal(|| None),
            print_job: use_signal(|| None),
            drawing: use_signal(|| None),
            style_clip: use_signal(|| None),
            editing_field: use_signal(|| None),
            radio_group: use_signal(|| None),
            windows: use_signal(|| [None, None]),
            groups: use_signal(|| None),
        }
    }

    #[track_caller]
    pub fn set_editing_field(&self, v: Option<(usize, usize)>) {
        note_write();
        *self.editing_field.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_radio_group(&self, v: Option<String>) {
        note_write();
        *self.radio_group.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_drawing(&self, v: Option<Drawing>) {
        note_write();
        *self.drawing.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_style_clip(&self, v: Option<folitecta_core::AnnotStyle>) {
        note_write();
        *self.style_clip.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_print_job(&self, v: Option<PrintJob>) {
        note_write();
        *self.print_job.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_clip(&self, v: Option<Clip>) {
        note_write();
        *self.clip.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_pending_image(&self, v: Option<Vec<u8>>) {
        note_write();
        *self.pending_image.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_loupe(&self, v: Option<LoupePos>) {
        note_write();
        *self.loupe.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_pointer(&self, v: Option<(usize, f32, f32)>) {
        note_write();
        *self.pointer.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_viewport(&self, v: (f64, f64)) {
        note_write();
        *self.viewport.write_unchecked() = v;
    }

    #[track_caller]
    pub fn set_dropdown_pos(&self, v: (f64, f64)) {
        note_write();
        *self.dropdown_pos.write_unchecked() = v;
    }

    #[track_caller]
    pub fn submenus_mut(&self) -> impl std::ops::DerefMut<Target = Vec<(String, f64, f64)>> + 'static {
        note_write();
        self.submenus.write_unchecked()
    }

    /// Clamp a floating menu so it stays inside the window (CSS pixels).
    pub fn clamp_menu_pos(&self, x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
        let (vw, vh) = *self.viewport.peek();
        let left = x.min((vw - width - 6.0).max(0.0)).max(0.0);
        let top = y.min((vh - height - 6.0).max(0.0)).max(0.0);
        (left, top)
    }

    #[track_caller]
    pub fn set_option(&self, id: &str, on: bool) {
        self.ui_mut().options.insert(id.to_string(), on);
        self.save_prefs();
    }

    /// A saved value of the interface (empty when unset).
    pub fn value(&self, key: &str) -> String {
        self.ui.read().value(key).unwrap_or_default().to_string()
    }

    #[track_caller]
    pub fn set_value(&self, key: &str, value: impl Into<String>) {
        self.ui_mut().values.insert(key.to_string(), value.into());
        self.save_prefs();
    }

    // Setters usable through a shared copy of the state (signals are handles).
    #[track_caller]
    pub fn set_dialog(&self, v: Dialog) {
        let leaves_mail = *self.dialog.peek() == Dialog::Mail && v != Dialog::Mail;
        note_write();
        *self.dialog.write_unchecked() = v;
        if leaves_mail {
            // The message of the e-mail dialog goes with it (its prepared files
            // are released, its work stopped).
            crate::commands::mail::forget();
        }
    }
    #[track_caller]
    pub fn set_palette(&self, v: Option<String>) {
        note_write();
        *self.palette.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_tool(&self, v: Tool) {
        note_write();
        *self.tool.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_active(&self, v: usize) {
        note_write();
        *self.active.write_unchecked() = v;
        crate::commands::tab_groups::sync(*self);
    }
    #[track_caller]
    pub fn set_fullscreen(&self, v: bool) {
        note_write();
        *self.fullscreen.write_unchecked() = v;
    }
    /// Open a menu of the menu bar (or none); the submenus of another menu close.
    #[track_caller]
    pub fn set_open_menu(&self, v: Option<String>) {
        if *self.open_menu.peek() != v {
            crate::ui::keep_submenus();
            if !self.submenus.peek().is_empty() {
                self.submenus_mut().clear();
            }
        }
        note_write();
        *self.open_menu.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_open_dropdown(&self, v: Option<String>) {
        if *self.open_dropdown.peek() != v {
            crate::ui::keep_submenus();
        }
        note_write();
        *self.open_dropdown.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_ribbon_tab(&self, v: String) {
        note_write();
        *self.ribbon_tab.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_canvas_size(&self, v: (f64, f64)) {
        note_write();
        *self.canvas_size.write_unchecked() = v;
    }

    /// Focus view `pane` of a split window: its page, zoom, layout and turn
    /// become those of the tab (and its canvas size the canvas size), the
    /// other view keeps those of before.
    pub fn focus_view(&self, pane: u8) {
        // The other group of tabs: its document becomes the active one.
        if crate::commands::tab_groups::focus_group(*self, usize::from(pane.min(1))) {
            return;
        }
        let size = *self.canvas_size.peek();
        let swapped = self
            .with_active(|t| {
                let mine = t.view_state();
                // The other document of a comparison is shown, not worked on.
                let s = t
                    .split
                    .as_mut()
                    .filter(|s| s.focus != pane && s.source.is_none())?;
                let other = std::mem::replace(&mut s.other, mine);
                let other_size = std::mem::replace(&mut s.other_size, size);
                s.focus = pane;
                t.set_view_state(other);
                Some(other_size)
            })
            .flatten();
        if let Some(size) = swapped {
            self.set_canvas_size(size);
        }
    }
    #[track_caller]
    pub fn set_dpr(&self, v: f64) {
        note_write();
        *self.dpr.write_unchecked() = v;
    }
    #[track_caller]
    pub fn set_busy(&self, v: bool) {
        note_write();
        *self.busy.write_unchecked() = v;
    }
    #[track_caller]
    pub fn docs_mut(&self) -> impl std::ops::DerefMut<Target = Vec<DocTab>> + 'static {
        note_write();
        self.docs.write_unchecked()
    }
    #[track_caller]
    pub fn ui_mut(&self) -> impl std::ops::DerefMut<Target = UiPrefs> + 'static {
        note_write();
        self.ui.write_unchecked()
    }
    #[track_caller]
    pub fn toasts_mut(&self) -> impl std::ops::DerefMut<Target = Vec<Toast>> + 'static {
        self.toasts.write_unchecked()
    }
    #[track_caller]
    pub fn closed_mut(&self) -> impl std::ops::DerefMut<Target = Vec<PathBuf>> + 'static {
        note_write();
        self.closed_recently.write_unchecked()
    }

    pub fn lang(&self) -> Lang {
        self.ui.read().lang
    }

    pub fn has_doc(&self) -> bool {
        !self.docs.read().is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        let n = self.docs.read().len();
        if n == 0 {
            None
        } else {
            Some((self.active)().min(n - 1))
        }
    }

    /// Run `f` on the active tab (mutably). Returns `None` without a document.
    #[track_caller]
    pub fn with_active<T>(&self, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
        let idx = self.active_index()?;
        let (out, shown) = {
            let mut docs = self.docs_mut();
            let tab = docs.get_mut(idx)?;
            let out = f(tab);
            tab.refresh();
            (out, tab.layer_shown.take())
        };
        if let Some(name) = shown {
            let lang = self.lang();
            self.toast(
                ToastKind::Info,
                crate::i18n::tf(lang, "toast.layer_shown", &[("name", &name)]),
                crate::i18n::t(lang, "toast.layer_shown_detail"),
            );
        }
        Some(out)
    }

    /// Read-only access to the active tab without following it: a component
    /// reading it this way is not drawn again when the documents change, so
    /// what it shows must come through its properties.
    pub fn peek_active<T>(&self, f: impl FnOnce(&DocTab) -> T) -> Option<T> {
        let docs = self.docs.peek();
        let n = docs.len();
        if n == 0 {
            return None;
        }
        let idx = (*self.active.peek()).min(n - 1);
        docs.get(idx).map(f)
    }

    /// Read-only access to the active tab.
    pub fn read_active<T>(&self, f: impl FnOnce(&DocTab) -> T) -> Option<T> {
        let idx = self.active_index()?;
        let docs = self.docs.read();
        docs.get(idx).map(f)
    }

    #[track_caller]
    pub fn alloc_id(&self) -> u64 {
        note_write();
        let mut n = self.next_id.write_unchecked();
        let id = *n;
        *n += 1;
        id
    }

    #[track_caller]
    pub fn toast(&self, kind: ToastKind, title: impl Into<String>, message: impl Into<String>) {
        let id = self.alloc_id();
        {
            let mut toasts = self.toasts_mut();
            toasts.push(Toast {
                id,
                kind,
                title: title.into(),
                message: message.into(),
            });
            // The latest messages only: a pile taller than the window would leave it.
            let extra = toasts.len().saturating_sub(4);
            toasts.drain(..extra);
        }
        let toasts = self.toasts;
        // On the root scope: a dialog that shows a message closes right away,
        // and a task of its own scope would stop with it.
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(std::time::Duration::from_millis(if kind == ToastKind::Error {
                9000
            } else {
                4500
            }))
            .await;
            toasts.write_unchecked().retain(|t| t.id != id);
        });
    }

    /// Show an error. The errors of the core about generated text (and
    /// problems of generated text themselves) are shown in the language of
    /// the interface.
    #[track_caller]
    pub fn error<E: std::fmt::Display + 'static>(&self, err: E) {
        let lang = self.lang();
        let title = crate::i18n::t(lang, "toast.error");
        let any = &err as &dyn std::any::Any;
        let message = if let Some(e) = any.downcast_ref::<folitecta_core::Error>() {
            crate::i18n::core_error(lang, e)
        } else if let Some(p) = any.downcast_ref::<folitecta_core::GlyphProblem>() {
            crate::i18n::glyph_problem(lang, p)
        } else {
            err.to_string()
        };
        self.toast(ToastKind::Error, title, message);
    }

    /// The text of an error of the core in the interface language (see
    /// [`crate::i18n::core_error`]).
    pub fn core_error_text(&self, e: &folitecta_core::Error) -> String {
        crate::i18n::core_error(self.lang(), e)
    }

    /// Show an error of the core (see [`AppState::core_error_text`]).
    pub fn core_error(&self, e: &folitecta_core::Error) {
        self.error(self.core_error_text(e));
    }

    #[track_caller]
    pub fn close_menus(&self) {
        // A submenu waiting to open must not open in a closed menu.
        crate::ui::keep_submenus();
        if self.open_menu.peek().is_some() {
            self.set_open_menu(None);
        }
        if self.open_dropdown.peek().is_some() {
            self.set_open_dropdown(None);
        }
        if !self.submenus.peek().is_empty() {
            self.submenus_mut().clear();
        }
    }

    pub fn save_prefs(&self) {
        self.ui.read().save();
    }

    /// Current page of the active document (0 when none).
    pub fn current_page(&self) -> usize {
        self.read_active(|t| t.page).unwrap_or(0)
    }

    pub fn page_count(&self) -> usize {
        self.read_active(|t| t.doc.page_count()).unwrap_or(0)
    }

    /// Navigate the active document to a page and scroll it into view.
    pub fn go_to_page(&self, page: usize) {
        self.navigate(page, true);
    }

    /// Show a destination: apply its zoom, scroll its top to the top of the
    /// window and briefly highlight the target area.
    #[track_caller]
    pub fn go_to_dest(&self, page: usize, view: Option<folitecta_core::DestView>) {
        use folitecta_core::DestView;
        let Some(size) = self.read_active(|t| t.page_sizes.get(page).copied()).flatten() else {
            return;
        };
        let (cw, ch) = *self.canvas_size.peek();
        match &view {
            Some(DestView::Fit) => {
                self.with_active(|t| t.zoom = Zoom::FitPage);
            }
            Some(DestView::FitWidth(_)) => {
                self.with_active(|t| t.zoom = Zoom::FitWidth);
            }
            Some(DestView::Xyz { zoom: Some(z), .. }) => {
                let z = z.clamp(0.1, 8.0);
                self.with_active(|t| t.zoom = Zoom::Fixed(z));
            }
            Some(DestView::Rect(r)) => {
                let (w, h) = ((r[2] - r[0]).abs().max(10.0), (r[3] - r[1]).abs().max(10.0));
                let z = ((cw as f32 - 56.0) / w)
                    .min((ch as f32 - 70.0) / h)
                    .clamp(0.1, 8.0);
                self.with_active(|t| t.zoom = Zoom::Fixed(z));
            }
            _ => {}
        }
        self.navigate(page, true);
        let c = size.crop_box;
        let band = match &view {
            Some(DestView::Xyz { top: Some(top), .. }) | Some(DestView::FitWidth(Some(top))) => {
                Some([c[0], top - 28.0, c[2], *top])
            }
            Some(DestView::Rect(r)) => Some(*r),
            _ => None,
        };
        let Some(band) = band else { return };
        let display = folitecta_core::marks::pdf_rect_to_display(&size, band);
        self.with_active(|t| t.flash = Some((page, display)));
        let st = *self;
        // On the root scope, like the messages: the caller may be a dialog that closes.
        dioxus::core::spawn_forever(async move {
            // Let the new zoom lay the pages out before scrolling inside the page.
            tokio::time::sleep(std::time::Duration::from_millis(160)).await;
            let zoom = crate::commands::current_zoom(st);
            let y = (display[1] * zoom - 16.0).max(0.0);
            let js = format!(
                "const el = document.getElementById('pg-{page}'); const c = document.getElementById('canvas'); if (el && c) {{ c.scrollTop += el.getBoundingClientRect().top - c.getBoundingClientRect().top + {y}; }}"
            );
            let _ = document::eval(&js).await;
            tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
            st.with_active(|t| {
                if t.flash.map(|(p, _)| p) == Some(page) {
                    t.flash = None;
                }
            });
        });
    }

    /// Show `page`; `record` adds the jump to the previous/next view history.
    #[track_caller]
    pub fn navigate(&self, page: usize, record: bool) {
        let n = self.page_count();
        if n == 0 {
            return;
        }
        let page = page.min(n - 1);
        self.with_active(|t| {
            if record && t.page != page {
                let from = t.page;
                t.push_history(from);
                t.push_history(page);
            }
            t.page = page;
        });
        let inline = if crate::commands::option_on(*self, "view.layout.ribbon") {
            "start"
        } else {
            "nearest"
        };
        let js = format!(
            "const el = document.getElementById('pg-{page}'); if (el) {{ el.scrollIntoView({{ block: 'start', inline: '{inline}' }}); }}"
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
    }
}
