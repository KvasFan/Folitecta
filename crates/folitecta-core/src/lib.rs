//! Folitecta core library.
//!
//! The core is independent from any user interface. It wraps a PDF object model
//! (lopdf) with an undo-aware [`Document`], a CPU rasterizer service built on
//! hayro ([`render`]), and the command [`registry`](commands) that describes every
//! user-facing command of the application.

pub mod annots;
pub mod attachments;
pub mod commands;
pub mod document;
pub mod error;
pub mod export;
pub mod forms;
pub mod layers;
pub mod links;
pub mod marks;
pub mod outline;
pub mod pages;
pub mod render;
pub mod text;
pub mod util;

pub use annots::{Annotation, Markup, Shape};
pub use attachments::Attachment;
pub use commands::{Command, CommandKind, Registry, Status, registry};
pub use document::{DocInfo, Document, PageSize};
pub use error::{Error, Result};
pub use export::ImageFormat;
pub use forms::{FieldKind, FormField, SignatureInfo};
pub use layers::Layer;
pub use links::{Link, LinkTarget, NamedDestination};
pub use marks::{MarkPosition, TextMarkOptions};
pub use outline::OutlineItem;
pub use render::{RenderService, RenderedPage, Renderer};
pub use text::{SearchHit, SearchOptions};

/// Crate version, shared with the application.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Product name.
pub const APP_NAME: &str = "Folitecta";
