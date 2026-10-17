//! Application state shared through the Dioxus context.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use folitecta_core::{CommentInfo, Document, PageSize, RenderService, SearchHit};
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
    EditAnnots,
    Zoom,
    Loupe,
    Note,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Highlight,
    TextBox,
    Typewriter,
    Ink,
    Link,
    Stamp,
    Redact,
    PlaceImage,
    TextHighlight,
    TextUnderline,
    TextStrike,
    Snapshot,
    Polygon,
    Polyline,
    Cloud,
    Callout,
    Distance,
    Perimeter,
    Area,
    FreeHighlight,
    Eraser,
    InsertText,
    ReplaceText,
    FileAttachment,
    EditFields,
    FieldText,
    FieldCheck,
    FieldRadio,
    FieldCombo,
    FieldList,
    FieldButton,
    FieldSignature,
    /// Draw the rectangle of a digital signature.
    SignArea,
    /// Place a signature, initials or a profile text.
    PlaceSignature,
}

impl Tool {
    /// Tools that create something by dragging or clicking on the page.
    pub fn is_drawing(&self) -> bool {
        !matches!(
            self,
            Tool::Hand | Tool::Select | Tool::EditAnnots | Tool::Zoom | Tool::Loupe | Tool::EditFields
        )
    }

    /// Tools that work on the text layer.
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Tool::Select | Tool::TextHighlight | Tool::TextUnderline | Tool::TextStrike | Tool::ReplaceText
        )
    }

    pub fn command_id(&self) -> &'static str {
        match self {
            Tool::Hand => "tool.hand",
            Tool::Select => "tool.selectText",
            Tool::EditAnnots => "tool.editAnnots",
            Tool::Zoom => "tool.zoom",
            Tool::Loupe => "tool.loupe",
            Tool::Note => "tool.annot.stickyNote",
            Tool::Rect => "tool.annot.square",
            Tool::Ellipse => "tool.annot.circle",
            Tool::Line => "tool.annot.line",
            Tool::Arrow => "tool.annot.arrow",
            Tool::Highlight => "tool.annot.highlightArea",
            Tool::TextBox => "tool.annot.textBox",
            Tool::Typewriter => "tool.annot.typeWriter",
            Tool::Ink => "tool.annot.pencil",
            Tool::Link => "tool.annot.editLink",
            Tool::Stamp => "tool.annot.stamp",
            Tool::Redact => "tool.annot.redaction",
            Tool::PlaceImage => "tool.selImagePlace",
            Tool::TextHighlight => "tool.annot.highlight",
            Tool::TextUnderline => "tool.annot.underline",
            Tool::TextStrike => "tool.annot.strikeout",
            Tool::Snapshot => "tool.snapshot",
            Tool::Polygon => "tool.annot.polygon",
            Tool::Polyline => "tool.annot.polyline",
            Tool::Cloud => "tool.annot.cloud",
            Tool::Callout => "tool.annot.callout",
            Tool::Distance => "tool.annot.distance",
            Tool::Perimeter => "tool.annot.perimeter",
            Tool::Area => "tool.annot.area",
            Tool::FreeHighlight => "tool.annot.freeHighlight",
            Tool::Eraser => "tool.annot.eraser",
            Tool::InsertText => "tool.annot.insertText",
            Tool::ReplaceText => "tool.annot.replaceText",
            Tool::FileAttachment => "tool.annot.fileAttachment",
            Tool::EditFields => "tool.editFields.mode.select",
            Tool::FieldText => "tool.editFields.mode.text",
            Tool::FieldCheck => "tool.editFields.mode.check",
            Tool::FieldRadio => "tool.editFields.mode.radio",
            Tool::FieldCombo => "tool.editFields.mode.combo",
            Tool::FieldList => "tool.editFields.mode.list",
            Tool::FieldButton => "tool.editFields.mode.button",
            Tool::FieldSignature => "tool.editFields.mode.signature",
            Tool::SignArea => "tool.digisig",
            Tool::PlaceSignature => "tool.sign.and.initials",
        }
    }

    /// i18n key of the tool name for the status bar.
    pub fn label_key(&self) -> &'static str {
        match self {
            Tool::Hand => "tool.hand",
            Tool::Select => "tool.select",
            Tool::EditAnnots => "tool.edit_annots",
            Tool::Zoom => "tool.zoom",
            Tool::Loupe => "tool.loupe",
            Tool::Note => "tool.note",
            Tool::Rect => "tool.rect",
            Tool::Ellipse => "tool.ellipse",
            Tool::Line => "tool.line",
            Tool::Arrow => "tool.arrow",
            Tool::Highlight => "tool.highlight",
            Tool::TextBox => "tool.text",
            Tool::Typewriter => "tool.typewriter",
            Tool::Ink => "tool.ink",
            Tool::Link => "tool.link",
            Tool::Stamp => "tool.stamp",
            Tool::Redact => "tool.redact",
            Tool::PlaceImage => "tool.image",
            Tool::TextHighlight => "tool.text_highlight",
            Tool::TextUnderline => "tool.text_underline",
            Tool::TextStrike => "tool.text_strike",
            Tool::Snapshot => "tool.snapshot",
            Tool::Polygon => "tool.polygon",
            Tool::Polyline => "tool.polyline",
            Tool::Cloud => "tool.cloud",
            Tool::Callout => "tool.callout",
            Tool::Distance => "tool.distance",
            Tool::Perimeter => "tool.perimeter",
            Tool::Area => "tool.area",
            Tool::FreeHighlight => "tool.free_highlight",
            Tool::Eraser => "tool.eraser",
            Tool::InsertText => "tool.insert_text",
            Tool::ReplaceText => "tool.replace_text",
            Tool::FileAttachment => "tool.file_attachment",
            Tool::EditFields => "tool.edit_fields",
            Tool::FieldText => "tool.field_text",
            Tool::FieldCheck => "tool.field_check",
            Tool::FieldRadio => "tool.field_radio",
            Tool::FieldCombo => "tool.field_combo",
            Tool::FieldList => "tool.field_list",
            Tool::FieldButton => "tool.field_button",
            Tool::FieldSignature => "tool.field_signature",
            Tool::SignArea => "tool.sign_area",
            Tool::PlaceSignature => "tool.place_signature",
        }
    }

    /// The kind of form field a field tool creates.
    pub fn new_field(&self) -> Option<folitecta_core::NewField> {
        use folitecta_core::NewField as F;
        Some(match self {
            Tool::FieldText => F::Text,
            Tool::FieldCheck => F::CheckBox,
            Tool::FieldRadio => F::RadioButton,
            Tool::FieldCombo => F::ComboBox,
            Tool::FieldList => F::ListBox,
            Tool::FieldButton => F::PushButton,
            Tool::FieldSignature => F::Signature,
            _ => return None,
        })
    }

    /// Tools that place points with clicks and finish with a double click or Enter.
    pub fn is_multi_click(&self) -> bool {
        matches!(
            self,
            Tool::Polygon | Tool::Polyline | Tool::Cloud | Tool::Perimeter | Tool::Area
        )
    }
}

/// Internal clipboard for comments.
#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    /// A comment of an open document, copied by reference.
    Annotation { tab: u64, page: usize, index: usize },
    /// A comment that was cut (kept as a detached copy).
    Detached(folitecta_core::AnnotSnapshot),
}

/// A text selection on one page: character offsets in the page text layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSel {
    pub page: usize,
    pub anchor: usize,
    pub head: usize,
}

impl TextSel {
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// A ruler guide in display points: horizontal guides store y, vertical ones x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    /// `None` = shown on every page.
    pub page: Option<usize>,
    pub horizontal: bool,
    pub pos: f32,
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
#[serde(default)]
pub struct UiPrefs {
    pub theme: Theme,
    pub lang: Lang,
    pub ribbon: bool,
    pub author: String,
    pub show_left_pane: bool,
    pub show_right_pane: bool,
    pub left_pane: String,
    pub recent: Vec<PathBuf>,
    /// On/off view options keyed by command id (rulers, grid, dark pages, ...).
    pub options: std::collections::BTreeMap<String, bool>,
    /// Valued settings keyed by name (ruler units, thumbnail size, ...).
    pub values: std::collections::BTreeMap<String, String>,
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
            options: Default::default(),
            values: Default::default(),
        }
    }
}

/// Per-user configuration directory without extra dependencies.
/// `FOLITECTA_CONFIG_DIR` overrides it (used by the interface self-test).
/// Folder of Folitecta's own data (settings, digital IDs, trusted certificates).
pub fn app_data_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("Folitecta"))
}

fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FOLITECTA_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
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

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn push_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }
}

/// Which comments are shown (a view setting: the document does not change).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommentFilter {
    pub hidden_authors: BTreeSet<String>,
    pub hidden_types: BTreeSet<String>,
    /// Some(true): only checked comments; Some(false): only unchecked ones.
    pub checked: Option<bool>,
    /// Only comments with this review state ("" = without a state).
    pub status: Option<String>,
}

impl CommentFilter {
    pub fn is_empty(&self) -> bool {
        *self == CommentFilter::default()
    }

    /// Whether a top-level comment passes the filter.
    pub fn shows(&self, c: &CommentInfo) -> bool {
        !self.hidden_authors.contains(&c.author)
            && !self.hidden_types.contains(&c.subtype)
            && self.checked.is_none_or(|k| c.marked == k)
            && self.status.as_ref().is_none_or(|s| &c.review == s)
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
    /// Current text selection (characters of one page).
    pub text_sel: Option<TextSel>,
    /// Ruler guides (view only, not saved in the PDF).
    pub guides: Vec<Guide>,
    /// Visited pages for previous/next view navigation.
    pub history: Vec<usize>,
    pub history_pos: usize,
    /// Comments shown on the pages and in the comments pane.
    pub comment_filter: CommentFilter,
    /// Bumped when the displayed pages change without a new revision (filters).
    pub view_gen: u64,
    /// Selected bookmark in the bookmarks pane.
    pub selected_bookmark: Option<Vec<usize>>,
    /// Selected named destination.
    pub selected_dest: Option<String>,
    /// Briefly highlighted area of a page (display points), e.g. a destination.
    pub flash: Option<(usize, [f32; 4])>,
    /// Signature validation results for a revision.
    pub sig_status: Option<(u64, Vec<folitecta_core::digsig::SignatureStatus>)>,
}

impl DocTab {
    pub fn new(id: u64, doc: Document) -> Self {
        // Bookmarks saved as open start expanded.
        fn open_keys(
            items: &[folitecta_core::OutlineItem],
            prefix: &mut Vec<usize>,
            out: &mut HashSet<String>,
        ) {
            for (i, item) in items.iter().enumerate() {
                prefix.push(i);
                if item.open && !item.children.is_empty() {
                    out.insert(prefix.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("."));
                }
                open_keys(&item.children, prefix, out);
                prefix.pop();
            }
        }
        let mut expanded = HashSet::new();
        open_keys(&doc.outline(), &mut Vec::new(), &mut expanded);
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
            expanded_bookmarks: expanded,
            page_sizes,
            revision,
            highlight_fields: false,
            text_sel: None,
            guides: Vec::new(),
            history: vec![0],
            history_pos: 0,
            comment_filter: CommentFilter::default(),
            view_gen: 0,
            selected_bookmark: None,
            selected_dest: None,
            flash: None,
            sig_status: None,
        }
    }

    /// Record a navigation to `page` in the view history.
    pub fn push_history(&mut self, page: usize) {
        if self.history.get(self.history_pos) == Some(&page) {
            return;
        }
        self.history.truncate(self.history_pos + 1);
        self.history.push(page);
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.history_pos = self.history.len() - 1;
    }

    /// Call after any mutation of `doc`: refreshes caches and the renderer.
    pub fn refresh(&mut self) {
        if self.doc.revision() != self.revision {
            self.page_sizes = self.doc.page_sizes();
            self.revision = self.doc.revision();
            self.update_render();
            let n = self.doc.page_count();
            if self.page >= n {
                self.page = n.saturating_sub(1);
            }
            self.selected_pages.retain(|p| *p < n);
            self.selected_annot = None;
            self.text_sel = None;
            // Search rectangles refer to the previous revision.
            self.search.hits.clear();
        }
    }

    /// Top-level comments the filter hides.
    pub fn filtered_out(&self) -> HashSet<(u32, u16)> {
        if self.comment_filter.is_empty() {
            return HashSet::new();
        }
        self.doc
            .comment_infos()
            .into_iter()
            .filter(|c| c.reply_to.is_none() && !c.is_state && !self.comment_filter.shows(c))
            .map(|c| c.id)
            .collect()
    }

    /// Give the renderer the document as displayed (comments hidden by the filter).
    pub fn update_render(&mut self) {
        let bytes = if self.comment_filter.is_empty() {
            self.doc.bytes()
        } else {
            self.doc
                .bytes_hiding(&self.filtered_out())
                .unwrap_or_else(|_| self.doc.bytes())
        };
        self.render.replace(bytes);
    }

    /// Change the comment filter and redraw the pages.
    pub fn set_comment_filter(&mut self, filter: CommentFilter) {
        if filter != self.comment_filter {
            self.comment_filter = filter;
            self.view_gen += 1;
            self.update_render();
        }
    }

    /// Revision of the displayed pages, for page image addresses.
    pub fn view_rev(&self) -> u64 {
        self.revision
            .wrapping_mul(1000)
            .wrapping_add(self.view_gen % 1000)
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
    ApplyRedactions(Option<usize>),
    RemoveScripts,
    /// Delete these pages.
    DeletePages(Vec<usize>),
    /// Remove every digital signature.
    ClearSignatures,
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
        /// Text box with a border; typewriter text has none.
        border: bool,
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
    NewBlank,
    PasteSpecial,
    Rasterize,
    StampPicker,
    Security,
    Scripts,
    Flatten,
    AddGuides,
    SplitTiles,
    Background,
    PageLabels,
    Transitions,
    ReplacePages(PathBuf),
    OverlayFile(PathBuf),
    /// Print the active document (`false`) or every open document (`true`).
    Print(bool),
    /// Text of a new callout pointing at `tip` with its box at `rect` (PDF space).
    Callout {
        page: usize,
        tip: (f32, f32),
        rect: [f32; 4],
    },
    /// Text to insert before (or after) character `index` of a page.
    InsertText {
        page: usize,
        index: usize,
        after: bool,
    },
    /// Replacement for the characters `start..end` of a page.
    ReplaceText {
        page: usize,
        start: usize,
        end: usize,
    },
    /// Scale of measurement comments.
    Calibrate,
    /// Options of a comment summary document.
    SummarizeComments,
    /// Reply to a comment.
    Reply {
        page: usize,
        index: usize,
    },
    /// Review state and check mark of a comment.
    ReviewStatus {
        page: usize,
        index: usize,
    },
    /// State changes of a comment.
    ReviewHistory {
        page: usize,
        index: usize,
    },
    /// Which comments are shown.
    CommentFilter,
    /// Options of the comments pane.
    CommentsViewOptions,
    /// Name and target of a named destination.
    NamedDestEdit(String),
    /// Options of the bookmarks and destinations panes.
    BookmarkOptions,
    /// Import and export of bookmarks.
    BookmarkImportExport,
    /// Target, colour and style of a bookmark.
    BookmarkProps(Vec<usize>),
    /// Which toolbars the classic interface shows.
    Toolbars,
    /// Zoom level.
    ZoomLevel,
    /// Sign or certify the document (certification level, invisible, field to sign).
    Sign {
        certify: u8,
        invisible: bool,
        field: Option<String>,
    },
    /// Details of a signature.
    SignatureDetails(String),
    /// Digital IDs: import, create, export, delete.
    DigitalIds,
    /// Trusted certificates.
    TrustedCerts,
    /// Library of signatures and initials, and the signer's profile.
    SignatureLibrary,
    /// List of the macros of page-mark texts.
    MacroGuide,
    /// External application launched with details of the view.
    LaunchApp,
}

/// Loupe position: page, x and y in display points, pointer client x and y.
pub type LoupePos = (usize, f32, f32, f64, f64);

/// Points placed so far by a multi-click tool: page and display points.
pub type Drawing = (usize, Vec<(f32, f32)>);

/// One printed page: tab id, page index, document revision.
pub type PrintPage = (u64, usize, u64);

/// Pages being printed through the hidden print view.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    pub pages: Vec<PrintPage>,
    /// Render scale in thousandths (1000 = 72 dpi).
    pub scale_milli: u32,
    pub annotations: bool,
    /// Scale each page to the paper; otherwise print at actual size.
    pub fit: bool,
    pub id: u64,
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
        }
    }

    pub fn set_editing_field(&self, v: Option<(usize, usize)>) {
        *self.editing_field.write_unchecked() = v;
    }

    pub fn set_radio_group(&self, v: Option<String>) {
        *self.radio_group.write_unchecked() = v;
    }

    pub fn set_drawing(&self, v: Option<Drawing>) {
        *self.drawing.write_unchecked() = v;
    }

    pub fn set_style_clip(&self, v: Option<folitecta_core::AnnotStyle>) {
        *self.style_clip.write_unchecked() = v;
    }

    pub fn set_print_job(&self, v: Option<PrintJob>) {
        *self.print_job.write_unchecked() = v;
    }

    pub fn set_clip(&self, v: Option<Clip>) {
        *self.clip.write_unchecked() = v;
    }
    pub fn set_pending_image(&self, v: Option<Vec<u8>>) {
        *self.pending_image.write_unchecked() = v;
    }
    pub fn set_loupe(&self, v: Option<LoupePos>) {
        *self.loupe.write_unchecked() = v;
    }
    pub fn set_pointer(&self, v: Option<(usize, f32, f32)>) {
        *self.pointer.write_unchecked() = v;
    }

    pub fn set_viewport(&self, v: (f64, f64)) {
        *self.viewport.write_unchecked() = v;
    }

    pub fn set_dropdown_pos(&self, v: (f64, f64)) {
        *self.dropdown_pos.write_unchecked() = v;
    }

    pub fn submenus_mut(&self) -> impl std::ops::DerefMut<Target = Vec<(String, f64, f64)>> + 'static {
        self.submenus.write_unchecked()
    }

    /// Clamp a floating menu so it stays inside the window (CSS pixels).
    pub fn clamp_menu_pos(&self, x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
        let (vw, vh) = *self.viewport.peek();
        let left = x.min((vw - width - 6.0).max(0.0)).max(0.0);
        let top = y.min((vh - height - 6.0).max(0.0)).max(0.0);
        (left, top)
    }

    pub fn set_option(&self, id: &str, on: bool) {
        self.ui_mut().options.insert(id.to_string(), on);
        self.save_prefs();
    }

    pub fn set_value(&self, key: &str, value: impl Into<String>) {
        self.ui_mut().values.insert(key.to_string(), value.into());
        self.save_prefs();
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
        spawn(async move {
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
