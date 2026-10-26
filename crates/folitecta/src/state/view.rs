//! How documents are viewed and worked on: zoom, layout, tools, text selections, guides and panes.

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
    /// Three clicks: the angle at the middle one.
    Angle,
    /// A drag from the centre of a circle to its edge.
    Radius,
    /// An area, then its depth.
    Volume,
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
    /// Draw the rectangle of a new table.
    Table,
    /// Draw the rectangle of a new barcode.
    Barcode,
    /// Draw the area of a page whose barcodes are read.
    BarcodeScan,
    Calculator,
    /// Place tick marks with clicks.
    Tick,
    ZoomDyn,
    Sound,
    Video,
    /// Select the pictures of the pages to edit them.
    EditImages,
    /// Select the vector paths of the pages to move their nodes, recolour
    /// or delete them.
    EditPaths,
    /// Select the text of the pages to change it (delete, type over, insert).
    EditText,
    /// Select text to suggest corrections with comments.
    TextCorrection,
    /// Draw the area of a page whose text is read (OCR).
    OcrRegion,
}

impl Tool {
    /// Tools that create something by dragging or clicking on the page.
    pub fn is_drawing(&self) -> bool {
        !matches!(
            self,
            Tool::Hand
                | Tool::Select
                | Tool::EditAnnots
                | Tool::Zoom
                | Tool::ZoomDyn
                | Tool::Loupe
                | Tool::EditFields
                | Tool::EditImages
                | Tool::EditPaths
                | Tool::EditText
                | Tool::TextCorrection
        )
    }

    /// Tools that work on the text layer.
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Tool::Select
                | Tool::TextHighlight
                | Tool::TextUnderline
                | Tool::TextStrike
                | Tool::ReplaceText
                | Tool::EditText
                | Tool::TextCorrection
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
            Tool::Angle => "tool.annot.angle",
            Tool::Radius => "tool.annot.radius",
            Tool::Volume => "tool.annot.volume",
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
            Tool::Table => "tool.table.create",
            Tool::Barcode => "tool.selBarcodePlace",
            Tool::BarcodeScan => "document.detectBarcodes",
            Tool::Calculator => "comments.accountingCalc",
            Tool::Tick => "tool.annot.tickMark",
            Tool::ZoomDyn => "tool.zoomDyn",
            Tool::Sound => "tool.annot.sound",
            Tool::Video => "tool.annot.editVideo",
            Tool::EditImages => "tool.editContent.images",
            Tool::EditPaths => "tool.editContent.shapes",
            Tool::EditText => "tool.editContent.text",
            Tool::TextCorrection => "tool.textCorrectionMarkup",
            Tool::OcrRegion => "document.OCRSelRgn",
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
            Tool::Angle => "tool.angle",
            Tool::Radius => "tool.radius",
            Tool::Volume => "tool.volume",
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
            Tool::Table => "tool.table",
            Tool::Barcode => "tool.barcode",
            Tool::BarcodeScan => "tool.barcode_scan",
            Tool::Calculator => "tool.calculator",
            Tool::Tick => "tool.tick",
            Tool::ZoomDyn => "tool.zoom_dyn",
            Tool::Sound => "tool.sound",
            Tool::Video => "tool.video",
            Tool::EditImages => "tool.edit_images",
            Tool::EditPaths => "tool.edit_paths",
            Tool::EditText => "tool.edit_text",
            Tool::TextCorrection => "tool.text_correction",
            Tool::OcrRegion => "tool.ocr_region",
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
            Tool::Polygon
                | Tool::Polyline
                | Tool::Cloud
                | Tool::Perimeter
                | Tool::Area
                | Tool::Angle
                | Tool::Volume
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
    History,
    Sessions,
    Stamps,
    Comparison,
    Tags,
    Order,
    Content,
    AccChecker,
    AccReport,
    Explorer,
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
            Pane::History => "history".into(),
            Pane::Sessions => "sessions".into(),
            Pane::Stamps => "stamps".into(),
            Pane::Comparison => "comparison".into(),
            Pane::Tags => "tags".into(),
            Pane::Order => "order".into(),
            Pane::Content => "content".into(),
            Pane::AccChecker => "accessibility-checker".into(),
            Pane::AccReport => "accessibility-report".into(),
            Pane::Explorer => "explorer".into(),
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
            "history" => Pane::History,
            "sessions" => Pane::Sessions,
            "stamps" => Pane::Stamps,
            "comparison" => Pane::Comparison,
            "tags" => Pane::Tags,
            "order" => Pane::Order,
            "content" => Pane::Content,
            "accessibility-checker" => Pane::AccChecker,
            "accessibility-report" => Pane::AccReport,
            "explorer" => Pane::Explorer,
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
