//! Folitecta core library.
//!
//! The core is independent from any user interface. It wraps a PDF object model
//! (lopdf) with an undo-aware [`Document`], a CPU rasterizer service built on
//! hayro ([`render`]), and the command [`registry`](commands) that describes every
//! user-facing command of the application.

pub mod annot_copy;
pub mod annots;
pub mod attachments;
pub mod commands;
pub mod document;
pub mod error;
pub mod export;
pub mod flatten;
pub mod forms;
pub mod images;
pub mod layers;
pub mod links;
pub mod marks;
pub mod outline;
pub mod pages;
pub mod pagetools;
pub mod redaction;
pub mod render;
pub mod scripts;
pub mod security;
pub mod stamps;
pub mod text;
pub mod textdoc;
pub mod textlayer;
pub mod textops;
pub mod util;

pub use annot_copy::AnnotSnapshot;
pub use annots::{Annotation, Markup, Shape};
pub use attachments::Attachment;
pub use commands::{Command, CommandKind, Registry, Status, registry};
pub use document::{DocInfo, Document, PageSize};
pub use error::{Error, Result};
pub use export::ImageFormat;
pub use flatten::FlattenScope;
pub use forms::{FieldKind, FormField, SignatureInfo};
pub use layers::Layer;
pub use links::{Link, LinkTarget, NamedDestination};
pub use marks::{MarkPosition, TextMarkOptions};
pub use outline::OutlineItem;
pub use pagetools::{Sides, TRANSITIONS};
pub use render::{RenderOpts, RenderService, RenderedPage, Renderer};
pub use scripts::ScriptInfo;
pub use security::SecurityPermissions;
pub use stamps::STAMP_PRESETS;
pub use text::{SearchHit, SearchOptions};
pub use textdoc::TextLayout;
pub use textlayer::{PageText, TextGlyph, TextLine};

/// Crate version, shared with the application.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Product name.
pub const APP_NAME: &str = "Folitecta";
