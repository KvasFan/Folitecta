//! File operations with native dialogs, by area: documents (open, save, close), export of
//! pages and text, picture files, form data, comment exchange, printing, pages from another PDF
//! and settings. Every function is reachable as `crate::files::<name>`.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use folitecta_core::{Document, Error as CoreError, ImageFormat};

use crate::i18n::{t, tf};
use crate::state::{AfterUnsaved, AppState, Dialog, DocTab, PasswordTarget, ToastKind};

mod comments;
mod documents;
mod export;
mod forms;
mod images;
mod pages;
mod print;
mod settings;

pub use comments::*;
pub use documents::*;
pub use export::*;
pub use forms::*;
pub use images::*;
pub use pages::*;
pub use print::*;
pub use settings::*;

pub(crate) fn pdf_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter("PDF documents", &["pdf"])
        .add_filter("All files", &["*"])
}

/// The dialog of File › Open: PDF documents, and e-mail messages made into documents.
pub(crate) fn open_file_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter("PDF documents", &["pdf"])
        .add_filter("E-mail messages", &["eml"])
        .add_filter("All files", &["*"])
}
