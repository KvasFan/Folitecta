//! Folitecta core library.
//!
//! The core is independent from any user interface. It wraps a PDF object model
//! (lopdf) with an undo-aware [`Document`], a CPU rasterizer service built on
//! hayro ([`render`]), and the command [`registry`](commands) that describes every
//! user-facing command of the application.
//!
//! The modules are grouped by area: [`organize`] (pages and documents),
//! [`markup`] (comments), [`graphics`] (pictures and barcodes), [`page_text`],
//! [`interactive`] (forms and scripts), [`navigation`], [`protection`] and
//! [`tags`]; the document model, rendering, the registry and the shared
//! helpers ([`util`], [`geom`], [`xml`], [`zip`], [`fonts`]) stay at the root. Every
//! module is also reachable from the root (`folitecta_core::pages`, ...).

pub mod commands;
pub mod document;
pub mod error;
pub mod fonts;
pub mod geom;
pub mod render;
pub mod util;
pub mod xml;
pub mod zip;

pub mod graphics;
pub mod interactive;
pub mod markup;
pub mod navigation;
pub mod organize;
pub mod page_text;
pub mod protection;
pub mod tags;

pub use graphics::{barcodes, images, imaging, pictures};
pub use interactive::{buttons, form_calc, form_data, form_design, forms, scripts};
pub use markup::{
    annot_copy, annots, appearance, arrange, calculator, comment_exchange, comment_review, comment_summary,
    flatten, grouping, markup_tools, media, signmarks, smooth, stamp_design, stamps, tables,
};
pub use navigation::{layers, links, outline};
pub use organize::{
    attachments, combine, compare, delivery, doctools, email, export, marks, metadata, pages, pagetools,
    portfolio, print, textdoc,
};
pub(crate) use page_text::{content_edit, content_lex};
pub use page_text::{ocr, search, spelling, text, textedit, textlayer, textops};
pub use protection::{digsig, redaction, security};
pub use tags::{accessibility, structure};

pub use annot_copy::AnnotSnapshot;
pub use annots::{Annotation, Markup, Shape};
pub use appearance::{AnnotStyle, LINE_ENDINGS, NOTE_ICONS};
pub use attachments::Attachment;
pub use barcodes::{BarcodeKind, FoundBarcode};
pub use buttons::{ButtonBehaviour, ButtonState};
pub use commands::{Command, CommandKind, Registry, Status, registry};
pub use comment_exchange::{CommentFormat, LeftOutReply, UndrawableComment};
pub use comment_review::{CommentInfo, REVIEW_STATES, StateChange};
pub use comment_summary::{SummaryEntry, SummaryLabels, SummaryLayout, SummaryOptions, SummarySort};
pub use delivery::{CopyComments, CopyFacts, CopyOptions, CopySignatures, CopySource, LeftOut, PreparedCopy};
pub use document::{DocInfo, Document, PageSize};
pub use error::{Error, Result};
pub use export::ImageFormat;
pub use flatten::FlattenScope;
pub use fonts::{AppearanceFont, GlyphProblem};
pub use form_calc::{CalcOp, Calculation};
pub use form_data::{FormDataFormat, PageWidget};
pub use form_design::{FieldProps, NewField};
pub use forms::{FieldKind, FormField, SignatureInfo};
pub use layers::{Layer, LayerNode};
pub use links::{Link, LinkTarget, NamedDestination};
pub use marks::{MACROS, MarkPosition, TextMarkOptions};
pub use markup_tools::{MeasureKind, MeasureScale};
pub use ocr::{OcrEngine, OcrLine, OcrModels, OcrOptions, OcrWord, PageOcr};
pub use outline::{BookmarkTarget, DestView, OutlineItem};
pub use pagetools::{Sides, TRANSITIONS};
pub use render::{RenderOpts, RenderService, RenderedPage, Renderer};
pub use scripts::ScriptInfo;
pub use security::{EncryptionInfo, EncryptionProfile, PasswordIssue, Protection, SecurityPermissions};
pub use signmarks::SignatureMark;
pub use spelling::{DictionaryFile, Misspelling, SpellOptions, SpellPlace, SpellScope, Speller};
pub use stamps::STAMP_PRESETS;
pub use text::{SearchHit, SearchOptions};
pub use textdoc::TextLayout;
pub use textlayer::{PageText, TextGlyph, TextLine};

/// Crate version, shared with the application.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Product name.
pub const APP_NAME: &str = "Folitecta";
