//! Action sequences (FOL-L047): named lists of steps — rotating pages,
//! deleting empty pages, recompressing pictures, colours to grey, comments
//! flattened or deleted, scripts removed, a watermark, page numbers, the
//! document information, PDF/A — kept as JSON, run on the open document as
//! one undo step or on many files. A batch is planned first (every input
//! with its output and any collision), then run file by file, each with a
//! result and a log; the window and the command line run the same code.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::{MarkPosition, TextMarkOptions};
use crate::recompress::{RecompressPreset, RecompressScope};

/// Which pages a step changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageSet {
    #[default]
    All,
    Odd,
    Even,
}

impl PageSet {
    fn pages(self, count: usize) -> Vec<usize> {
        (0..count)
            .filter(|p| match self {
                PageSet::All => true,
                PageSet::Odd => p % 2 == 0,
                PageSet::Even => p % 2 == 1,
            })
            .collect()
    }
}

fn default_opacity() -> f32 {
    0.3
}

fn default_position() -> String {
    "bottom-center".into()
}

/// One step of a sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Step {
    Rotate {
        degrees: i32,
        #[serde(default)]
        pages: PageSet,
    },
    DeleteEmptyPages,
    /// A preset of [`RecompressPreset`] by its id (`screen`, `ebook`, `print`, `archive`).
    Recompress {
        preset: String,
    },
    Grey,
    FlattenComments,
    DeleteComments,
    RemoveScripts,
    Watermark {
        text: String,
        #[serde(default = "default_opacity")]
        opacity: f32,
    },
    /// `format` with the macros of page marks (`{page}`, `{pages}`, ...);
    /// `position` such as `bottom-center`.
    PageNumbers {
        format: String,
        #[serde(default = "default_position")]
        position: String,
    },
    /// The document information; empty fields are left as they are.
    Info {
        #[serde(default)]
        title: String,
        #[serde(default)]
        author: String,
        #[serde(default)]
        subject: String,
        #[serde(default)]
        keywords: String,
    },
    /// Convert to PDF/A-2b (declared only when every rule checked is met).
    PdfA,
}

impl Step {
    /// The ids of the steps, in the order they are offered.
    pub const IDS: [&'static str; 11] = [
        "rotate",
        "delete-empty-pages",
        "recompress",
        "grey",
        "flatten-comments",
        "delete-comments",
        "remove-scripts",
        "watermark",
        "page-numbers",
        "info",
        "pdf-a",
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Step::Rotate { .. } => "rotate",
            Step::DeleteEmptyPages => "delete-empty-pages",
            Step::Recompress { .. } => "recompress",
            Step::Grey => "grey",
            Step::FlattenComments => "flatten-comments",
            Step::DeleteComments => "delete-comments",
            Step::RemoveScripts => "remove-scripts",
            Step::Watermark { .. } => "watermark",
            Step::PageNumbers { .. } => "page-numbers",
            Step::Info { .. } => "info",
            Step::PdfA => "pdf-a",
        }
    }

    /// A step of `id` with its usual settings.
    pub fn new(id: &str) -> Option<Step> {
        Some(match id {
            "rotate" => Step::Rotate {
                degrees: 90,
                pages: PageSet::All,
            },
            "delete-empty-pages" => Step::DeleteEmptyPages,
            "recompress" => Step::Recompress {
                preset: "ebook".into(),
            },
            "grey" => Step::Grey,
            "flatten-comments" => Step::FlattenComments,
            "delete-comments" => Step::DeleteComments,
            "remove-scripts" => Step::RemoveScripts,
            "watermark" => Step::Watermark {
                text: "DRAFT".into(),
                opacity: default_opacity(),
            },
            "page-numbers" => Step::PageNumbers {
                format: "{page} / {pages}".into(),
                position: default_position(),
            },
            "info" => Step::Info {
                title: String::new(),
                author: String::new(),
                subject: String::new(),
                keywords: String::new(),
            },
            "pdf-a" => Step::PdfA,
            _ => return None,
        })
    }

    /// Run the step on `doc`; a line for the log.
    fn apply(&self, doc: &mut Document) -> Result<String> {
        let n = doc.page_count();
        Ok(match self {
            Step::Rotate { degrees, pages } => {
                if degrees % 90 != 0 {
                    return Err(Error::Invalid(format!(
                        "a rotation of {degrees}° is not a quarter turn"
                    )));
                }
                let list = pages.pages(n);
                doc.rotate_pages(&list, *degrees)?;
                format!("rotated {} pages by {degrees}°", list.len())
            }
            Step::DeleteEmptyPages => {
                let empty = doc.empty_pages();
                if empty.len() >= n {
                    "every page is empty: none deleted".to_string()
                } else if empty.is_empty() {
                    "no empty page".to_string()
                } else {
                    doc.delete_pages(&empty)?;
                    format!("deleted {} empty pages", empty.len())
                }
            }
            Step::Recompress { preset } => {
                let p = RecompressPreset::from_id(preset)
                    .ok_or_else(|| Error::Invalid(format!("unknown preset {preset:?}")))?;
                let r = doc.recompress(&RecompressScope::All, p.settings())?;
                format!(
                    "recompressed {} pictures: {} → {} bytes",
                    r.images, r.before, r.after
                )
            }
            Step::Grey => {
                let r = doc.convert_to_grey()?;
                format!(
                    "grey: {} colours, {} pictures, {} comments and fields",
                    r.colours, r.pictures, r.comments
                )
            }
            Step::FlattenComments => {
                let all: Vec<usize> = (0..n).collect();
                doc.flatten_comments(&all)?;
                "comments flattened".to_string()
            }
            Step::DeleteComments => {
                let k = doc.delete_all_comments(None)?;
                format!("deleted {k} comments")
            }
            Step::RemoveScripts => {
                let k = doc.remove_scripts()?;
                format!("removed {k} scripts")
            }
            Step::Watermark { text, opacity } => {
                if text.trim().is_empty() {
                    return Err(Error::Invalid("the watermark has no text".into()));
                }
                let opts = TextMarkOptions {
                    text: text.clone(),
                    opacity: opacity.clamp(0.05, 1.0),
                    ..TextMarkOptions::default()
                };
                doc.add_text_watermark(&(0..n).collect::<Vec<_>>(), &opts)?;
                format!("watermark {text:?} on {n} pages")
            }
            Step::PageNumbers { format, position } => {
                doc.add_page_numbers(
                    &(0..n).collect::<Vec<_>>(),
                    format,
                    1,
                    position_of(position),
                    10.0,
                    24.0,
                    [0.0, 0.0, 0.0],
                )?;
                format!("page numbers {format:?} on {n} pages")
            }
            Step::Info {
                title,
                author,
                subject,
                keywords,
            } => {
                let info = doc.info();
                let pick = |new: &str, old: &str| {
                    if new.trim().is_empty() {
                        old.to_string()
                    } else {
                        new.to_string()
                    }
                };
                doc.set_info(
                    &pick(title, &info.title),
                    &pick(author, &info.author),
                    &pick(subject, &info.subject),
                    &pick(keywords, &info.keywords),
                )?;
                "document information set".to_string()
            }
            Step::PdfA => {
                let outcome = doc.make_pdfa()?;
                if outcome.claimed {
                    "converted to PDF/A-2b".to_string()
                } else {
                    let left: Vec<&str> = outcome
                        .after
                        .broken()
                        .into_iter()
                        .map(|(r, _)| r)
                        .filter(|r| *r != "identification")
                        .collect();
                    format!("not PDF/A: {} still broken", left.join(", "))
                }
            }
        })
    }
}

/// A position name (`top-left`, `bottom-center`, ...) as a [`MarkPosition`].
pub fn position_of(name: &str) -> MarkPosition {
    match name {
        "center" => MarkPosition::Center,
        "top-left" => MarkPosition::TopLeft,
        "top-center" => MarkPosition::TopCenter,
        "top-right" => MarkPosition::TopRight,
        "bottom-left" => MarkPosition::BottomLeft,
        "bottom-right" => MarkPosition::BottomRight,
        _ => MarkPosition::BottomCenter,
    }
}

/// A named list of steps.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ActionSequence {
    pub name: String,
    pub steps: Vec<Step>,
}

impl ActionSequence {
    pub fn from_json(text: &str) -> Result<ActionSequence> {
        serde_json::from_str(text).map_err(|e| Error::Invalid(format!("not an action sequence: {e}")))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

impl Document {
    /// Run `seq` on the document, step by step, as one undo step. The log has
    /// a line per step done; at the first step that fails the document goes
    /// back to how it was and the error comes with the log so far.
    pub fn run_actions(
        &mut self,
        seq: &ActionSequence,
    ) -> std::result::Result<Vec<String>, (Vec<String>, Error)> {
        let mark = self.undo_mark();
        let before = self.revision();
        let mut log = Vec::new();
        for (i, step) in seq.steps.iter().enumerate() {
            match step.apply(self) {
                Ok(line) => log.push(format!("{}. {line}", i + 1)),
                Err(e) => {
                    if self.revision() != before {
                        self.squash_undo(&mark);
                        let _ = self.undo();
                    }
                    log.push(format!("{}. {}: {e}", i + 1, step.id()));
                    return Err((log, e));
                }
            }
        }
        if self.revision() != before {
            self.squash_undo(&mark);
        }
        Ok(log)
    }
}

/// Where the outputs of a batch go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputRule {
    /// A folder for every output (None: next to each input).
    pub folder: Option<PathBuf>,
    /// Added to the name of each input.
    pub suffix: String,
    /// Existing files (or the input itself) are replaced.
    pub overwrite: bool,
}

impl Default for OutputRule {
    fn default() -> Self {
        OutputRule {
            folder: None,
            suffix: "-processed".into(),
            overwrite: false,
        }
    }
}

/// One file of a batch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchItem {
    pub input: PathBuf,
    pub output: PathBuf,
    /// `exists` (the output is there), `same-file` (the output is the
    /// input), `twice` (an earlier input has the same output).
    pub collision: Option<&'static str>,
}

/// The PDF files of `paths`: files as they are, folders searched with their
/// subfolders; sorted, each once.
pub fn batch_inputs(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() && depth < 32 {
                walk(&p, out, depth + 1);
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, &mut out, 0);
        } else {
            out.push(p.clone());
        }
    }
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// Every input with its output and any collision.
pub fn plan_batch(inputs: &[PathBuf], rule: &OutputRule) -> Vec<BatchItem> {
    let mut used: HashSet<PathBuf> = HashSet::new();
    inputs
        .iter()
        .map(|input| {
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let name = format!("{stem}{}.pdf", rule.suffix);
            let folder = rule
                .folder
                .clone()
                .or_else(|| input.parent().map(Path::to_path_buf))
                .unwrap_or_default();
            let output = folder.join(name);
            let collision = if !used.insert(output.clone()) {
                Some("twice")
            } else if &output == input {
                Some("same-file")
            } else if output.exists() {
                Some("exists")
            } else {
                None
            };
            BatchItem {
                input: input.clone(),
                output,
                collision,
            }
        })
        .collect()
}

/// What happened to one file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileResult {
    pub input: PathBuf,
    pub output: PathBuf,
    /// `done`, `failed` or `skipped`.
    pub status: &'static str,
    pub message: String,
    pub log: Vec<String>,
}

/// Run `seq` on one file of a batch and write its output.
pub fn run_batch_item(seq: &ActionSequence, item: &BatchItem, rule: &OutputRule) -> FileResult {
    let result = |status, message: String, log| FileResult {
        input: item.input.clone(),
        output: item.output.clone(),
        status,
        message,
        log,
    };
    match item.collision {
        Some("twice") => return result("skipped", "another input has the same output".into(), Vec::new()),
        Some(c) if !rule.overwrite => {
            let why = if c == "same-file" {
                "the output would replace the input"
            } else {
                "the output exists"
            };
            return result("skipped", why.into(), Vec::new());
        }
        _ => {}
    }
    let mut doc = match Document::open(&item.input) {
        Ok(d) => d,
        Err(e) => {
            let why = if matches!(e, Error::PasswordRequired | Error::WrongPassword) {
                "the file is protected by a password".to_string()
            } else {
                e.to_string()
            };
            return result("failed", why, Vec::new());
        }
    };
    let log = match doc.run_actions(seq) {
        Ok(log) => log,
        Err((log, e)) => return result("failed", e.to_string(), log),
    };
    if let Some(dir) = item.output.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return result("failed", e.to_string(), log);
        }
    }
    match doc.save_as(&item.output) {
        Ok(()) => result("done", String::new(), log),
        Err(e) => result("failed", e.to_string(), log),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_read_and_write_json() {
        let seq = ActionSequence {
            name: "Archive".into(),
            steps: Step::IDS.iter().map(|id| Step::new(id).unwrap()).collect(),
        };
        let json = seq.to_json();
        assert!(json.contains("\"action\": \"delete-empty-pages\""), "{json}");
        assert_eq!(ActionSequence::from_json(&json).unwrap(), seq);
        let short = ActionSequence::from_json(r#"{"name":"x","steps":[{"action":"rotate","degrees":180},{"action":"watermark","text":"COPY"}]}"#).unwrap();
        assert_eq!(
            short.steps[0],
            Step::Rotate {
                degrees: 180,
                pages: PageSet::All
            }
        );
        assert_eq!(
            short.steps[1],
            Step::Watermark {
                text: "COPY".into(),
                opacity: 0.3
            }
        );
        assert!(ActionSequence::from_json(r#"{"name":"x","steps":[{"action":"format-disk"}]}"#).is_err());
        assert_eq!(PageSet::Odd.pages(5), vec![0, 2, 4]);
        assert_eq!(PageSet::Even.pages(5), vec![1, 3]);
    }
}
