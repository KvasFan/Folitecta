//! Modal dialogs, confirmations, messages and print jobs.

use super::*;

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
    RemoveScripts,
    /// Delete these pages.
    DeletePages(Vec<usize>),
    /// Remove every digital signature.
    ClearSignatures,
    /// Tag the document again (its tags are replaced).
    AutoTag,
    /// Remove the tags of the document.
    ClearStructure,
    /// Move the document file to the recycle bin.
    TrashDocument,
    /// Move the files selected in the explorer to the recycle bin.
    TrashFiles,
    /// Import the comments of this file that can be drawn, leaving out the others.
    ImportCommentsSkipping(std::sync::Arc<Vec<u8>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterUnsaved {
    Close,
    CloseAll,
    Exit,
}

/// What the password of the password dialog opens.
#[derive(Debug, Clone, PartialEq)]
pub enum PasswordTarget {
    /// A file, in a new tab.
    File(PathBuf),
    /// A file in place of the document of tab `tab` (Revert); the tab stays
    /// as it is until the file is open.
    Replace { path: PathBuf, tab: u64 },
    /// The revision a signature of the document of tab `tab` covers.
    Revision { tab: u64, field: String },
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
    /// A password, for `target`; `retry` after a wrong one.
    Password {
        target: PasswordTarget,
        retry: bool,
    },
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
    Flatten,
    AddGuides,
    /// Cut lines over a page, then the pages cut along them.
    SplitPages,
    Background,
    PageLabels,
    Transitions,
    ReplacePages(PathBuf),
    OverlayFile(PathBuf),
    /// The print window.
    Print(PrintTarget),
    /// Text of a new callout pointing at `tip` with its box at `rect` (PDF space).
    Callout {
        page: usize,
        tip: (f32, f32),
        rect: [f32; 4],
    },
    /// Text to insert before (or after) character `index` of a page: in the
    /// text of the page, or (`note`) as a correction comment.
    InsertText {
        page: usize,
        index: usize,
        after: bool,
        note: bool,
    },
    /// Replacement for the characters `start..end` of a page: in the text of
    /// the page, or (`note`) as a correction comment.
    ReplaceText {
        page: usize,
        start: usize,
        end: usize,
        note: bool,
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
    /// Add a document time stamp from a time stamp server.
    TimeStamp,
    /// Options of signature validation (online revocation checks).
    SignatureOptions,
    /// Add validation material (certificates, revocation data) to the document.
    ValidationMaterial,
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
    /// Name of a new layer or group (`group`), or of the entry at `rename`.
    LayerName {
        group: bool,
        rename: Option<Vec<usize>>,
    },
    /// Show or hide every layer in the view.
    LayerOverrides,
    /// Merge the selected layers into a chosen one.
    LayerMerge,
    /// Draw the pages of a file as a new layer.
    ImportLayer(PathBuf),
    /// Options of the layers pane.
    LayerOptions,
    /// The layer new drawings and text go to, or the layer of the selected comments.
    ChooseLayer,
    /// Name of the new layer the selected comments or picture move to.
    MoveToNewLayer,
    /// Name, description, relationship and contents of an attachment.
    AttachmentEdit(usize),
    /// Columns of the attachments pane.
    AttachmentOptions,
    /// Options of the fields pane.
    FieldsOptions,
    /// New portfolio folder (`None`) or a folder's name and description.
    PortfolioFolder {
        id: Option<i64>,
    },
    /// Description and column values of a portfolio file.
    PortfolioItem(Vec<u8>),
    /// Layout, initial file, sort order and columns of a portfolio.
    PortfolioProps,
    /// Name of a new session (`None`: save the open documents) or of a saved one.
    SessionName(Option<usize>),
    /// Values of the custom fields of a stamp before placing it.
    StampValues(u64),
    /// A new text stamp (`None`) or a stamp's name, text and colour.
    StampText(Option<u64>),
    /// A new stamp collection (`None`) or a collection's name.
    StampGroup(Option<usize>),
    /// Options of the stamps pane.
    StampOptions,
    /// New colour of a stamp.
    StampRecolor(u64),
    /// The stamp designer.
    StampDesigner,
    /// Pages with page marks of a kind (Watermark, HeaderFooter, PageNumber).
    ManageMarks(String),
    /// Space used by each kind of content.
    SpaceAudit,
    /// Recompress the pictures of the document (or of the current page).
    Recompress(bool),
    /// Pages of typed text inserted after the current page.
    InsertTextPages,
    /// The selected signature repeated on other pages.
    MultiPlace,
    /// Scripts run on document events.
    DocumentActions,
    /// Choose the documents to compare.
    Compare,
    /// Rows and columns of a new table drawn at `rect` (PDF space).
    TableNew {
        page: usize,
        rect: [f32; 4],
    },
    /// The table editor.
    TableEditor,
    /// Copies of a form field in rows and columns.
    FieldCopies((u32, u16)),
    /// A new tag inside `parent` (the root when None) at `index` among its kids.
    TagNew {
        parent: Option<(u32, u16)>,
        index: Option<usize>,
    },
    /// Properties of a tag.
    TagProps((u32, u16)),
    /// A tag made from an area of a page (display space).
    TagFromArea {
        page: usize,
        rect: [f32; 4],
    },
    RoleMap,
    ClassMap,
    /// Alternate text of the figures.
    AltText,
    /// Options of an accessibility pane (its id).
    AccOptions(String),
    /// What a rule of the accessibility check means.
    RuleExplain(String),
    /// Primary language of the document.
    DocLanguage,
    /// Settings of new calculator tapes.
    CalcSettings,
    /// New name of the document file (its current name).
    RenameDoc(String),
    /// Metadata templates and the XMP properties of the document.
    MetaTemplates,
    /// Keep the document information as a template.
    MetaTemplateSave,
    /// JavaScript console.
    JsConsole,
    /// JavaScript preferences.
    JsPrefs,
    /// Named scripts of the document (Some: start a new one).
    DocScripts(Option<String>),
    /// Sound and video comments of the document.
    MediaList,
    /// Properties of a sound or video comment.
    MediaProps {
        page: usize,
        index: usize,
    },
    /// A new folder in the explorer.
    NewFolder,
    /// A custom folder filter (None: a new one).
    FolderFilter(Option<usize>),
    /// Source files of a combined document.
    Combine,
    /// The picture editor.
    ImageEditor,
    /// A picture being edited in another app.
    ExternalPicture,
    /// How pictures are edited: the editor, the file type, every drawing.
    PictureSettings,
    /// The name of a named comment style.
    StyleName(usize),
    /// How redacted areas look once applied.
    RedactionLook,
    /// A new redaction code, or (set) a new set of codes.
    RedactionCode {
        set: bool,
    },
    /// Apply the redaction marks of a page (or of every page): what goes,
    /// what is sanitized, and whether the result is saved and checked.
    ApplyRedactions {
        page: Option<usize>,
    },
    /// Find and Replace in the text of the pages, starting with `find`.
    FindReplace {
        find: String,
    },
    /// Documents with unsaved changes left by a window that ended.
    Recovery,
    /// The kind and content of a barcode drawn at `rect` (display points, y down).
    Barcode {
        page: usize,
        rect: [f32; 4],
    },
    /// Barcodes read on the pages.
    Barcodes(Vec<folitecta_core::FoundBarcode>),
    /// The order in which calculated fields are calculated.
    CalcOrder,
    /// The icon of a note (page and index), or of the new notes.
    NoteIcon(Option<(usize, usize)>),
    /// Spell Check and Replace: the words the dictionaries do not know, one by one.
    SpellCheck,
    /// Text recognition of pages (or only of their pictures).
    Ocr {
        pictures_only: bool,
    },
    /// The text read in an area of a page.
    OcrText {
        text: String,
    },
    /// The depth of a volume measured over `points` (PDF user space) of `page`.
    VolumeDepth {
        page: usize,
        points: Vec<(f32, f32)>,
    },
    /// The mark the tick mark tool places.
    TickMarks,
    /// Bookmarks made of the headings found in the text.
    BookmarksFromHeadings,
    /// Pages listing the bookmarks inserted into the document.
    InsertContents,
    /// The words text recognition read with doubt, one at a time, after
    /// `skipped` words left doubtful.
    OcrReview {
        skipped: usize,
    },
    /// Clean Up Scans: blank and askew scanned pages.
    CleanUpScans,
    /// The gallery of templates of new documents.
    Templates,
    /// Save the active document as one of the user's templates.
    SaveTemplate,
    /// Scanning pages into a new document.
    Scan,
    /// Action sequences: editing them and running them.
    Actions,
    /// Export to office formats.
    ExportOffice,
    /// Preflight against PDF/A-2b and conversion.
    Preflight,
    /// Converting the colours of the document to grey.
    ConvertGrey,
    /// The items of the chosen combo box or list box.
    FieldItems,
    /// The list of security policies.
    SecurityPolicies,
    /// The commands of the Quick Access Toolbar.
    CustomizeQat,
    /// The files of an e-mail message (state in `commands::mail::MAIL`).
    Mail,
}

/// Loupe position: page, x and y in display points, pointer client x and y.
pub type LoupePos = (usize, f32, f32, f64, f64);

/// Points placed so far by a multi-click tool: page and display points.
pub type Drawing = (usize, Vec<(f32, f32)>);

/// What the print window prints.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintTarget {
    /// The active document.
    Active,
    /// These pages of the active document (zero-based).
    Pages(Vec<usize>),
    /// Every open document.
    AllOpen,
    /// PDF files given by name and contents (the files of a portfolio), with
    /// the protection of the document they come from (and the tab holding it).
    Files {
        files: Vec<PrintFile>,
        protection: Option<folitecta_core::Protection>,
        tab: Option<u64>,
    },
}

/// A PDF file to print: its name and contents.
pub type PrintFile = (String, std::sync::Arc<Vec<u8>>);

/// Sheets laid out for the system print dialog, as pictures.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    /// The generation of the sheets (part of their addresses).
    pub generation: u64,
    /// The number of sheets of each document.
    pub sheets: Vec<usize>,
    /// Render scale in thousandths (1000 = 72 dpi).
    pub scale_milli: u32,
    /// Width and height of the paper of the first sheet, in points.
    pub paper: (f32, f32),
}
