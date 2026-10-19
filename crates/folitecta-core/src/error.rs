use std::path::PathBuf;

/// Errors produced by the core library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error: {0}")]
    IoPlain(#[from] std::io::Error),
    #[error("the document is password protected")]
    PasswordRequired,
    #[error("the password is not right")]
    WrongPassword,
    /// The protection of the document cannot be applied to the file to
    /// write; nothing was written.
    #[error("the protection of the document cannot be kept ({0}); nothing was written")]
    ProtectionNotKept(String),
    /// Content of a protected file would go into a document that is not
    /// protected.
    #[error(
        "the content comes from a protected file and this document is not protected: it is not copied (save an unrestricted copy of that file first)"
    )]
    ProtectedSource,
    /// The file is protected in a way Folitecta cannot open (what, in words).
    #[error("the file is protected with {0}, which Folitecta cannot open yet")]
    UnsupportedProtection(String),
    #[error("page {0} does not exist")]
    InvalidPage(usize),
    #[error("render error: {0}")]
    Render(String),
    #[error("image encoding error: {0}")]
    Image(String),
    #[error("{0}")]
    Invalid(String),
    #[error("a document must keep at least one page")]
    LastPage,
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("document has no file path; use Save As")]
    NoPath,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
