//! Application state shared through the Dioxus context.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use folitecta_core::{Document, PageSize, RenderService, SearchHit};
use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    Fixed(f32),
    FitWidth,
    FitPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Single,
    Continuous,
    TwoPages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Hand,
    Select,
    Zoom,
    Note,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Highlight,
    TextBox,
    Ink,
    Link,
}

impl Tool {
    pub fn is_drawing(&self) -> bool {
        !matches!(self, Tool::Hand | Tool::Select | Tool::Zoom)
    }

    pub fn command_id(&self) -> &'static str {
        match self {
            Tool::Hand => "tool.hand",
            Tool::Select => "tool.selectText",
            Tool::Zoom => "tool.zoom",
            Tool::Note => "tool.annot.stickyNote",
            Tool::Rect => "tool.annot.square",
            Tool::Ellipse => "tool.annot.circle",
            Tool::Line => "tool.annot.line",
            Tool::Arrow => "tool.annot.arrow",
            Tool::Highlight => "tool.annot.highlightArea",
            Tool::TextBox => "tool.annot.textBox",
            Tool::Ink => "tool.annot.pencil",
            Tool::Link => "tool.annot.editLink",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pane {
    Thumbnails,
    Bookmarks,
    Comments,
    Attachments,
    Layers,
    Fields,
    Links,
    Destinations,
    Search,
    Signatures,
    Other(String),
}

impl Pane {
    pub fn id(&self) -> String {
        match self {
            Pane::Thumbnails => "thumbnails".into(),
            Pane::Bookmarks => "bookmarks".into(),
            Pane::Comments => "comments".into(),
            Pane::Attachments => "attachments".into(),
            Pane::Layers => "layers".into(),
            Pane::Fields => "fields".into(),
            Pane::Links => "links".into(),
            Pane::Destinations => "destinations".into(),
            Pane::Search => "search".into(),
            Pane::Signatures => "signatures".into(),
            Pane::Other(s) => s.clone(),
        }
    }

    pub fn from_id(id: &str) -> Pane {
        match id {
            "thumbnails" => Pane::Thumbnails,
            "bookmarks" => Pane::Bookmarks,
            "comments" => Pane::Comments,
            "attachments" => Pane::Attachments,
            "layers" => Pane::Layers,
            "fields" => Pane::Fields,
            "links" => Pane::Links,
            "destinations" => Pane::Destinations,
            "search" => Pane::Search,
            "signatures" => Pane::Signatures,
            other => Pane::Other(other.to_string()),
        }
    }

    pub const MAIN: [Pane; 10] = [
        Pane::Thumbnails,
        Pane::Bookmarks,
        Pane::Comments,
        Pane::Attachments,
        Pane::Layers,
        Pane::Fields,
        Pane::Links,
        Pane::Destinations,
        Pane::Search,
        Pane::Signatures,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiPrefs {
    pub theme: Theme,
    pub lang: Lang,
    pub ribbon: bool,
    pub author: String,
    pub show_left_pane: bool,
    pub show_right_pane: bool,
    pub left_pane: String,
    pub recent: Vec<PathBuf>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs {
            theme: Theme::Light,
            lang: Lang::En,
            ribbon: true,
            author: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "Author".into()),
            show_left_pane: true,
            show_right_pane: false,
            left_pane: "thumbnails".into(),
            recent: Vec::new(),
        }
    }
}

/// Per-user configuration directory without extra dependencies.
fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

impl UiPrefs {
    fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("Folitecta").join("settings.json"))
    }

    pub fn load() -> UiPrefs {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn push_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub current: usize,
    pub case_sensitive: bool,
    pub whole_words: bool,
}

/// One open document tab.
pub struct DocTab {
    pub id: u64,
    pub doc: Document,
    pub render: Arc<RenderService>,
    pub page: usize,
    pub zoom: Zoom,
    pub layout: Layout,
    pub view_rotation: i32,
    pub selected_pages: Vec<usize>,
    pub selected_annot: Option<(usize, usize)>,
    pub search: SearchState,
    pub expanded_bookmarks: HashSet<String>,
    pub page_sizes: Vec<PageSize>,
    pub revision: u64,
    pub highlight_fields: bool,
}

impl DocTab {
    pub fn new(id: u64, doc: Document) -> Self {
        let render = Arc::new(RenderService::new(doc.bytes()));
        let page_sizes = doc.page_sizes();
        let revision = doc.revision();
        DocTab {
            id,
            doc,
            render,
            page: 0,
            zoom: Zoom::FitWidth,
            layout: Layout::Continuous,
            view_rotation: 0,
            selected_pages: Vec::new(),
            selected_annot: None,
            search: SearchState::default(),
            expanded_bookmarks: HashSet::new(),
            page_sizes,
            revision,
            highlight_fields: false,
        }
    }

    /// Call after any mutation of `doc`: refreshes caches and the renderer.
    pub fn refresh(&mut self) {
        if self.doc.revision() != self.revision {
            self.page_sizes = self.doc.page_sizes();
            self.revision = self.doc.revision();
            self.render.replace(self.doc.bytes());
            let n = self.doc.page_count();
            if self.page >= n {
                self.page = n.saturating_sub(1);
            }
            self.selected_pages.retain(|p| *p < n);
            self.selected_annot = None;
        }
    }

    pub fn title(&self) -> String {
        self.doc.display_name()
    }

    /// Pages targeted by page operations: the selection, or the current page.
    pub fn target_pages(&self) -> Vec<usize> {
        if self.selected_pages.is_empty() {
            vec![self.page]
        } else {
            self.selected_pages.clone()
        }
    }

    /// Display size of a page in points, after the view rotation.
    pub fn display_size(&self, page: usize) -> (f32, f32) {
        let s = self.page_sizes.get(page).copied().unwrap_or(PageSize {
            width: 595.0,
            height: 842.0,
            rotation: 0,
            media_box: [0.0; 4],
            crop_box: [0.0; 4],
        });
        if self.view_rotation % 180 != 0 {
            (s.height, s.width)
        } else {
            (s.width, s.height)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteAllComments,
    RemoveLinks,
    ResetForm,
    Revert,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterUnsaved {
    Close,
    CloseAll,
    Exit,
}

/// Modal dialogs. Each variant carries the data the dialog needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Dialog {
    #[default]
    None,
    About,
    Shortcuts,
    Matrix,
    Prefs,
    CommandLine,
    DocProps,
    GoToPage,
    InsertBlank,
    RotatePages,
    DeletePages,
    ExtractPages,
    CropPages,
    ResizePages,
    AddMargins,
    MovePages,
    SwapPages,
    Watermark,
    HeaderFooter,
    PageNumbers,
    Bates,
    RemoveMarks,
    ExportImages,
    Split,
    MergeFile(PathBuf),
    NotImplemented(String),
    Confirm {
        title: String,
        body: String,
        action: ConfirmAction,
    },
    Unsaved {
        tab: u64,
        then: AfterUnsaved,
    },
    Password(PathBuf),
    Note {
        page: usize,
        x: f32,
        y: f32,
    },
    TextBox {
        page: usize,
        rect: [f32; 4],
    },
    Link {
        page: usize,
        rect: [f32; 4],
    },
    FieldValue {
        id: (u32, u16),
        name: String,
        value: String,
    },
    BookmarkNew,
    BookmarkRename(Vec<usize>),
    AnnotationText {
        page: usize,
        index: usize,
        contents: String,
    },
    WordCount {
        words: usize,
        chars: usize,
    },
    UrlReport(Vec<(usize, String)>),
    Summary(String),
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
    pub busy: Signal<bool>,
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
            busy: use_signal(|| false),
        }
    }

    // Setters usable through a shared copy of the state (signals are handles).
    pub fn set_dialog(&self, v: Dialog) {
        *self.dialog.write_unchecked() = v;
    }
    pub fn set_palette(&self, v: Option<String>) {
        *self.palette.write_unchecked() = v;
    }
    pub fn set_tool(&self, v: Tool) {
        *self.tool.write_unchecked() = v;
    }
    pub fn set_active(&self, v: usize) {
        *self.active.write_unchecked() = v;
    }
    pub fn set_fullscreen(&self, v: bool) {
        *self.fullscreen.write_unchecked() = v;
    }
    pub fn set_open_menu(&self, v: Option<String>) {
        *self.open_menu.write_unchecked() = v;
    }
    pub fn set_open_dropdown(&self, v: Option<String>) {
        *self.open_dropdown.write_unchecked() = v;
    }
    pub fn set_ribbon_tab(&self, v: String) {
        *self.ribbon_tab.write_unchecked() = v;
    }
    pub fn set_canvas_size(&self, v: (f64, f64)) {
        *self.canvas_size.write_unchecked() = v;
    }
    pub fn set_dpr(&self, v: f64) {
        *self.dpr.write_unchecked() = v;
    }
    pub fn set_busy(&self, v: bool) {
        *self.busy.write_unchecked() = v;
    }
    pub fn docs_mut(&self) -> impl std::ops::DerefMut<Target = Vec<DocTab>> + 'static {
        self.docs.write_unchecked()
    }
    pub fn ui_mut(&self) -> impl std::ops::DerefMut<Target = UiPrefs> + 'static {
        self.ui.write_unchecked()
    }
    pub fn toasts_mut(&self) -> impl std::ops::DerefMut<Target = Vec<Toast>> + 'static {
        self.toasts.write_unchecked()
    }
    pub fn closed_mut(&self) -> impl std::ops::DerefMut<Target = Vec<PathBuf>> + 'static {
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
    pub fn with_active<T>(&self, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
        let idx = self.active_index()?;
        let mut docs = self.docs_mut();
        let tab = docs.get_mut(idx)?;
        let out = f(tab);
        tab.refresh();
        Some(out)
    }

    /// Read-only access to the active tab.
    pub fn read_active<T>(&self, f: impl FnOnce(&DocTab) -> T) -> Option<T> {
        let idx = self.active_index()?;
        let docs = self.docs.read();
        docs.get(idx).map(f)
    }

    pub fn alloc_id(&self) -> u64 {
        let mut n = self.next_id.write_unchecked();
        let id = *n;
        *n += 1;
        id
    }

    pub fn toast(&self, kind: ToastKind, title: impl Into<String>, message: impl Into<String>) {
        let id = self.alloc_id();
        self.toasts_mut().push(Toast {
            id,
            kind,
            title: title.into(),
            message: message.into(),
        });
        let toasts = self.toasts;
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(if kind == ToastKind::Error {
                9000
            } else {
                4500
            }))
            .await;
            toasts.write_unchecked().retain(|t| t.id != id);
        });
    }

    pub fn error(&self, err: impl std::fmt::Display) {
        let title = crate::i18n::t(self.lang(), "toast.error");
        self.toast(ToastKind::Error, title, err.to_string());
    }

    pub fn close_menus(&self) {
        if self.open_menu.peek().is_some() {
            self.set_open_menu(None);
        }
        if self.open_dropdown.peek().is_some() {
            self.set_open_dropdown(None);
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
        let n = self.page_count();
        if n == 0 {
            return;
        }
        let page = page.min(n - 1);
        self.with_active(|t| t.page = page);
        let js = format!(
            "const el = document.getElementById('pg-{page}'); if (el) {{ el.scrollIntoView({{ block: 'start' }}); }}"
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
    }
}
