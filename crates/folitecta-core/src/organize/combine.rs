//! Documents made from several source files: PDF files (all their pages or
//! some), pictures (a page each) and text files (pages of text).

use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::incoming::Arrival;
use crate::textdoc::TextLayout;

/// Kinds of source files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Pdf,
    Image,
    Text,
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];
const TEXT_EXTS: &[&str] = &["txt", "text", "md", "csv", "log"];

/// Extensions of every file that can be combined.
pub fn source_exts() -> Vec<&'static str> {
    let mut out = vec!["pdf"];
    out.extend_from_slice(IMAGE_EXTS);
    out.extend_from_slice(TEXT_EXTS);
    out
}

/// The kind of a source file from its extension.
pub fn source_kind(path: &Path) -> Option<SourceKind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let ext = ext.as_str();
    if ext == "pdf" {
        Some(SourceKind::Pdf)
    } else if IMAGE_EXTS.contains(&ext) {
        Some(SourceKind::Image)
    } else if TEXT_EXTS.contains(&ext) {
        Some(SourceKind::Text)
    } else {
        None
    }
}

/// One source of a combined document: a file and, for PDF files, the pages
/// to take (zero-based; all when None).
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    pub pages: Option<Vec<usize>>,
}

/// The document a source file stands for.
pub fn source_document(path: &Path) -> Result<Document> {
    match source_kind(path) {
        Some(SourceKind::Pdf) => Document::open(path),
        Some(SourceKind::Image) => {
            let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;
            Document::from_images(vec![data])
        }
        Some(SourceKind::Text) => {
            let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;
            Document::from_text(&String::from_utf8_lossy(&data), TextLayout::default())
        }
        None => Err(Error::Invalid(format!("{} cannot be combined", path.display()))),
    }
}

/// Number of pages of a source file.
pub fn source_pages(path: &Path) -> Result<usize> {
    match source_kind(path) {
        Some(SourceKind::Image) => Ok(1),
        _ => source_document(path).map(|d| d.page_count()),
    }
}

/// A combined document, the sources that could not be read (file and
/// reason) and what each source that went in brought along.
pub type Combined = (Document, Vec<(PathBuf, String)>, Vec<(PathBuf, Arrival)>);

impl Document {
    /// A new document with the pages of the sources in order, each source
    /// under a bookmark named after its file with its own bookmarks, its
    /// fields, destinations, layers and (when the first file is tagged) its
    /// tags; what was renamed or left behind is in the [`Arrival`] of each
    /// source. Sources that cannot be read are left out and reported (file
    /// and reason), and so are protected files: the combined document is not
    /// protected, so their pages would be written plain.
    pub fn combine(sources: &[Source]) -> Result<(Document, Vec<(PathBuf, String)>)> {
        Self::combine_with_arrivals(sources).map(|(doc, problems, _)| (doc, problems))
    }

    /// [`combine`](Self::combine), with what each source brought along,
    /// renamed or left behind (sources that went in only).
    pub fn combine_with_arrivals(sources: &[Source]) -> Result<Combined> {
        if sources.is_empty() {
            return Err(Error::Invalid("no files to combine".into()));
        }
        let mut doc = Document::new_blank(1, 595.0, 842.0)?;
        let mut problems = Vec::new();
        let mut arrivals = Vec::new();
        for s in sources {
            let r = source_document(&s.path).and_then(|other| {
                let n = other.page_count();
                let pages = s
                    .pages
                    .as_ref()
                    .map(|p| p.iter().copied().filter(|i| *i < n).collect::<Vec<_>>());
                if pages.as_ref().is_some_and(|p| p.is_empty()) {
                    return Err(Error::Invalid("none of the chosen pages exists".into()));
                }
                let at = doc.page_count();
                // Tagged from the first file on, when that one is tagged.
                if at == 1 && arrivals.is_empty() && other.has_structure() {
                    doc.changing();
                    doc.ensure_struct_root()?;
                    doc.commit()?;
                }
                let title = s
                    .path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                doc.insert_pages_from(at, &other, pages.as_deref(), Some(&title))
            });
            match r {
                Ok(a) => arrivals.push((s.path.clone(), a)),
                Err(e) => problems.push((s.path.clone(), e.to_string())),
            }
        }
        if doc.page_count() == 1 {
            return Err(Error::Invalid(match problems.first() {
                Some((p, e)) => format!("{}: {e}", p.display()),
                None => "no pages to combine".into(),
            }));
        }
        // The blank page the document started with.
        doc.delete_pages(&[0])?;
        doc.set_path(None);
        doc.clear_history();
        Ok((doc, problems, arrivals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_of_sources() {
        assert_eq!(source_kind(Path::new("a/B.PDF")), Some(SourceKind::Pdf));
        assert_eq!(source_kind(Path::new("photo.jpeg")), Some(SourceKind::Image));
        assert_eq!(source_kind(Path::new("notes.txt")), Some(SourceKind::Text));
        assert_eq!(source_kind(Path::new("archive.zip")), None);
        assert!(
            source_exts()
                .iter()
                .all(|e| source_kind(Path::new(&format!("x.{e}"))).is_some())
        );
        assert!(Document::combine(&[]).is_err());
    }
}
