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
