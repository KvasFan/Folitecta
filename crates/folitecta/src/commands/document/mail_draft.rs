//! What the e-mail dialog prepares: the files of a message (one row for each
//! version of a document, its form data and its comments), their options and
//! names, what each preparation does and the work that runs it away from the
//! interface thread. Plain functions, tested without the interface.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::Engine;
use folitecta_core::delivery::PROTECTION_REFUSED;
use folitecta_core::{
    CommentFormat, CopyComments, CopyFacts, CopyOptions, CopySource, Document, Error, FormDataFormat,
};

use crate::i18n::{Lang, t, tf};
use crate::mail::{HandOverError, Outgoing};
use crate::util::{RangeError, parse_pages};

/// Data taken out of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    /// The values of the form fields.
    Form(FormDataFormat),
    /// The comments (XFDF or FDF).
    Comments(CommentFormat),
}

/// What a row attaches.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// The saved file, byte for byte (or changed, when the document holds no
    /// changes since it was saved).
    Saved(PathBuf),
    /// The document as it is now, with the changes not saved yet.
    Current,
    Data(DataKind),
}

/// Which pages of a PDF row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pages {
    All,
    Current,
    Range,
}

/// A file ready to be attached, with what it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub bytes: Arc<Vec<u8>>,
    /// Facts read back from the file (`None`: a protected file read from the
    /// disk that cannot be opened here).
    pub facts: Option<CopyFacts>,
    /// The first pages, as `data:` URLs of PNG pictures.
    pub thumbs: Vec<String>,
    /// Pages not shown.
    pub more_pages: usize,
    /// The beginning of a data file, as text.
    pub text: Option<String>,
    /// A PDF made by Folitecta (checked to be whole when it is written).
    pub made_here: bool,
}

/// Why a row cannot be attached.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    Range(RangeError),
    /// Form data or comments of a protected document would be plain text.
    DataOfProtected,
    /// The saved file of a protected document is not protected.
    SavedPlain,
    /// A copy of a protected document could not be protected.
    NotProtected,
    /// The document was closed.
    Closed,
    /// The document has no form fields to export.
    NoFields,
    /// The file could not be read.
    Read(String),
    Other(String),
}

impl Problem {
    /// What it says to the user.
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Problem::Range(RangeError::NotARange(part)) => {
                tf(lang, "dialog.mail.bad_range", &[("part", part)])
            }
            Problem::Range(RangeError::OutOfBounds(n)) => {
                tf(lang, "dialog.mail.range_bounds", &[("count", &n.to_string())])
            }
            Problem::DataOfProtected => t(lang, "dialog.mail.data_protected"),
            Problem::SavedPlain => t(lang, "dialog.mail.saved_plain"),
            Problem::NotProtected => t(lang, "dialog.mail.not_protected"),
            Problem::Closed => t(lang, "dialog.mail.closed"),
            Problem::NoFields => t(lang, "toast.no_form_fields"),
            Problem::Read(e) => tf(lang, "dialog.mail.read_failed", &[("reason", e)]),
            Problem::Other(e) => e.clone(),
        }
    }
}

fn core_problem(e: Error, protected: bool) -> Problem {
    match e {
        Error::Invalid(m) if protected && m == PROTECTION_REFUSED => Problem::NotProtected,
        e => Problem::Other(e.to_string()),
    }
}

/// How far a row is.
#[derive(Debug, Clone, PartialEq)]
pub enum Prep {
    /// To be prepared.
    Pending,
    /// Being prepared, for this revision of the row.
    Running(u64),
    Ready(Arc<Prepared>),
    Failed(Problem),
}

impl Prep {
    pub fn ready(&self) -> Option<&Arc<Prepared>> {
        match self {
            Prep::Ready(p) => Some(p),
            _ => None,
        }
    }
}

/// What the dialog knows of a document when it opens.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocFacts {
    pub tab: u64,
    pub title: String,
    /// The saved file, when it is there.
    pub saved: Option<PathBuf>,
    /// When it was saved (seconds since 1970).
    pub saved_at: u64,
    pub dirty: bool,
    pub pages: usize,
    pub current_page: usize,
    pub selected: Vec<usize>,
    pub comments: usize,
    /// Form fields with values to export.
    pub has_form: bool,
    pub protected: bool,
}

impl DocFacts {
    pub fn of(id: u64, doc: &Document, page: usize, selected: &[usize]) -> DocFacts {
        let saved = doc.path().filter(|p| p.is_file()).map(Path::to_path_buf);
        let saved_at = saved
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        DocFacts {
            tab: id,
            title: doc.display_name(),
            saved,
            saved_at,
            dirty: doc.is_dirty(),
            pages: doc.page_count(),
            current_page: page,
            selected: selected.to_vec(),
            comments: doc.comments().len(),
            has_form: doc.form_fields().iter().any(|f| {
                !matches!(
                    f.kind,
                    folitecta_core::FieldKind::PushButton | folitecta_core::FieldKind::Signature
                )
            }),
            protected: doc.is_protected(),
        }
    }

    /// The title without its extension.
    pub fn stem(&self) -> String {
        match self.title.rsplit_once('.') {
            Some((s, e)) if e.eq_ignore_ascii_case("pdf") => s.to_string(),
            _ => self.title.clone(),
        }
    }
}

/// One file of the message.
#[derive(Debug, Clone, PartialEq)]
pub struct MailItem {
    pub kind: ItemKind,
    pub doc: DocFacts,
    pub include: bool,
    pub name: String,
    pub pages: Pages,
    pub range: String,
    pub comments: CopyComments,
    /// Bumped by every change of what the row prepares.
    pub rev: u64,
    pub prep: Prep,
}

/// Where the hand-over to the mail program is.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Editing,
    /// The files or the draft are being written.
    Writing,
    /// Waiting for the mail program.
    Handing,
    /// A message opened in the mail program, with this many files: whether
    /// they are all in it cannot be checked.
    Opened(usize),
    Failed(HandOverError),
}

/// The message being prepared.
#[derive(Debug, Clone, PartialEq)]
pub struct MailDraft {
    pub id: u64,
    pub items: Vec<MailItem>,
    pub selected: usize,
    pub zip: bool,
    pub zip_name: String,
    pub zip_prep: Prep,
    /// Bumped by every change of what the archive holds.
    pub zip_key: u64,
    pub subject: String,
    /// The subject follows the names of the files until it is typed.
    pub subject_auto: bool,
    pub body: String,
    pub phase: Phase,
    /// The folder of the files handed to the mail program.
    pub outbox: Option<PathBuf>,
}

/// What the command asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Documents { all: bool, zip: bool },
    FormData,
}

/// A name every file system takes.
pub fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() { "document".into() } else { s }
}

/// `name` ending in `.ext` (added when it does not, in any case).
fn with_extension(name: &str, ext: &str) -> String {
    let dotted = format!(".{ext}");
    if name.to_lowercase().ends_with(&dotted) {
        name.to_string()
    } else {
        format!("{name}{dotted}")
    }
}

fn format_ext(kind: DataKind) -> &'static str {
    match kind {
        DataKind::Form(f) => f.extension(),
        DataKind::Comments(CommentFormat::Fdf) => "fdf",
        DataKind::Comments(_) => "xfdf",
    }
}

impl MailItem {
    /// Pages and comments can be chosen (not for the saved file of a document
    /// with changes since: it is attached as it is).
    pub fn transforms_allowed(&self) -> bool {
        match &self.kind {
            ItemKind::Current => true,
            ItemKind::Saved(_) => !self.doc.dirty,
            ItemKind::Data(_) => false,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self.kind {
            ItemKind::Data(k) => format_ext(k),
            _ => "pdf",
        }
    }

    /// The pages chosen: `None` for every page.
    pub fn page_list(&self) -> Result<Option<Vec<usize>>, Problem> {
        let n = self.doc.pages;
        let list = match self.pages {
            Pages::All => return Ok(None),
            Pages::Current => vec![self.doc.current_page.min(n.saturating_sub(1))],
            Pages::Range => {
                let mut v = parse_pages(&self.range, n).map_err(Problem::Range)?;
                v.sort_unstable();
                v.dedup();
                v
            }
        };
        Ok((list.len() != n).then_some(list))
    }

    /// What preparing the row does.
    pub fn plan(&self) -> Result<Plan, Problem> {
        match &self.kind {
            ItemKind::Data(_) if self.doc.protected => Err(Problem::DataOfProtected),
            ItemKind::Data(DataKind::Form(_)) if !self.doc.has_form => Err(Problem::NoFields),
            ItemKind::Data(k) => Ok(Plan::Data(*k)),
            ItemKind::Saved(p) if !self.transforms_allowed() => Ok(Plan::File(p.clone())),
            kind => {
                let options = CopyOptions {
                    pages: self.page_list()?,
                    comments: self.comments,
                };
                match kind {
                    ItemKind::Saved(p) if options == CopyOptions::default() => Ok(Plan::File(p.clone())),
                    _ => Ok(Plan::Copy(options)),
                }
            }
        }
    }
}

/// What preparing a row does.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// The saved file, byte for byte.
    File(PathBuf),
    /// A copy of the document as it is.
    Copy(CopyOptions),
    Data(DataKind),
}

/// The work of a row, with what it needs from the document.
pub enum Job {
    /// `same_as`: the document, when it holds exactly the saved file (to show
    /// a protected file that only opens with its password).
    File {
        path: PathBuf,
        same_as: Option<CopySource>,
        protected: bool,
    },
    Copy {
        source: CopySource,
        options: CopyOptions,
    },
    Data {
        bytes: Vec<u8>,
    },
}

static IDS: AtomicU64 = AtomicU64::new(1);

/// A new message for the documents of `docs` (the active one, or every open
/// one in tab order).
pub fn new_draft(lang: Lang, how: Start, docs: &[DocFacts]) -> MailDraft {
    let mut items = Vec::new();
    let row = |kind: ItemKind, doc: &DocFacts, include: bool, name: String| MailItem {
        kind,
        doc: doc.clone(),
        include,
        name,
        pages: Pages::All,
        range: crate::util::format_pages(&doc.selected),
        comments: CopyComments::Keep,
        rev: 0,
        prep: Prep::Pending,
    };
    let documents = !matches!(how, Start::FormData);
    for doc in docs {
        let stem = file_safe(&doc.stem());
        let data = row(
            ItemKind::Data(DataKind::Form(FormDataFormat::Xfdf)),
            doc,
            !documents,
            format!("{stem}-data.xfdf"),
        );
        if !documents && doc.has_form {
            items.push(data.clone());
        }
        match &doc.saved {
            Some(p) if doc.dirty => {
                items.push(row(ItemKind::Current, doc, documents, format!("{stem}.pdf")));
                let suffix = t(lang, "dialog.mail.saved_suffix");
                items.push(row(
                    ItemKind::Saved(p.clone()),
                    doc,
                    false,
                    format!("{stem} ({suffix}).pdf"),
                ));
            }
            Some(p) => {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("{stem}.pdf"));
                items.push(row(ItemKind::Saved(p.clone()), doc, documents, name));
            }
            None => items.push(row(ItemKind::Current, doc, documents, format!("{stem}.pdf"))),
        }
        if documents && doc.has_form {
            let mut data = data;
            data.include = false;
            items.push(data);
        }
        if doc.comments > 0 {
            items.push(row(
                ItemKind::Data(DataKind::Comments(CommentFormat::Xfdf)),
                doc,
                false,
                format!("{stem}-comments.xfdf"),
            ));
        }
    }
    let zip_name = match docs {
        [one] => format!("{}.zip", file_safe(&one.stem())),
        _ => format!("{}.zip", t(lang, "label.documents")),
    };
    let mut d = MailDraft {
        id: IDS.fetch_add(1, Ordering::SeqCst),
        items,
        selected: 0,
        zip: matches!(how, Start::Documents { zip: true, .. }),
        zip_name,
        zip_prep: Prep::Pending,
        zip_key: 0,
        subject: String::new(),
        subject_auto: true,
        body: String::new(),
        phase: Phase::Editing,
        outbox: None,
    };
    d.subject = default_subject(&d);
    d
}

/// The names the included rows are attached under: made safe, with their
/// extension, numbered when two are the same (in any case).
pub fn final_names(items: &[MailItem]) -> Vec<Option<String>> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .map(|it| {
            if !it.include {
                return None;
            }
            let base = with_extension(&file_safe(it.name.trim()), it.extension());
            let mut name = base.clone();
            let mut n = 1;
            while !seen.insert(name.to_lowercase()) {
                n += 1;
                name = match base.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                    None => format!("{base} ({n})"),
                };
            }
            Some(name)
        })
        .collect()
}

/// The name of the archive, with its extension.
pub fn zip_file_name(d: &MailDraft) -> String {
    with_extension(&file_safe(d.zip_name.trim()), "zip")
}

/// The subject made from the names of the included files.
pub fn default_subject(d: &MailDraft) -> String {
    final_names(&d.items)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

/// The included rows, each with its name and what it prepared, once every
/// one is ready.
pub fn prepared_rows(d: &MailDraft) -> Option<Vec<Outgoing>> {
    let names = final_names(&d.items);
    let mut out = Vec::new();
    for (it, name) in d.items.iter().zip(names) {
        let Some(name) = name else { continue };
        let p = it.prep.ready()?;
        out.push(Outgoing {
            name,
            bytes: p.bytes.clone(),
            made_here: p.made_here,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// The files to attach: the prepared rows, or one archive holding them.
/// `None` until everything is ready (or when nothing is included).
pub fn attachments(d: &MailDraft) -> Option<Vec<Outgoing>> {
    let rows = prepared_rows(d)?;
    if !d.zip {
        return Some(rows);
    }
    let zip = d.zip_prep.ready()?;
    Some(vec![Outgoing {
        name: zip_file_name(d),
        bytes: zip.bytes.clone(),
        made_here: false,
    }])
}

/// One ZIP archive of `files` (names numbered when two are the same).
pub fn zip_of(files: &[Outgoing]) -> Result<Vec<u8>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    for f in files {
        let mut entry = f.name.clone();
        let mut n = 1;
        while !seen.insert(entry.to_lowercase()) {
            n += 1;
            entry = match f.name.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                None => format!("{} ({n})", f.name),
            };
        }
        entries.push(folitecta_core::zip::ZipEntry::new(entry, f.bytes.to_vec()));
    }
    folitecta_core::zip::write(&entries).map_err(|e| e.to_string())
}

/// The rows to start preparing now: included, waiting, the selected one
/// first, so that no more than `limit` run at once. A row being prepared is
/// never started again; typing the subject, the message or a name changes
/// no row, so it starts nothing.
pub fn rows_to_start(d: &MailDraft, limit: usize) -> Vec<usize> {
    let running = d
        .items
        .iter()
        .filter(|it| matches!(it.prep, Prep::Running(_)))
        .count();
    let mut order: Vec<usize> = (0..d.items.len()).collect();
    order.sort_by_key(|&i| i != d.selected);
    order
        .into_iter()
        .filter(|&i| d.items[i].include && d.items[i].prep == Prep::Pending)
        .take(limit.saturating_sub(running))
        .collect()
}

/// The archive is to be made: it is asked for, every included row is ready
/// and it is not made yet.
pub fn zip_wanted(d: &MailDraft) -> bool {
    d.zip && d.zip_prep == Prep::Pending && prepared_rows(d).is_some()
}

/// Mark row `i` as changed: it is prepared again, and so is the archive.
pub fn touch(d: &mut MailDraft, i: usize) {
    if let Some(it) = d.items.get_mut(i) {
        it.rev += 1;
        it.prep = Prep::Pending;
    }
    touch_zip(d);
}

/// The archive is made again (its files or their names changed).
pub fn touch_zip(d: &mut MailDraft) {
    d.zip_key += 1;
    d.zip_prep = Prep::Pending;
    if d.subject_auto {
        d.subject = default_subject(d);
    }
}

/// The text of the first lines of a data file.
fn first_lines(bytes: &[u8], lines: usize) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Largest number of pages shown.
pub const THUMBS: usize = 4;

/// Pictures of the first pages of `plain`, as `data:` URLs.
fn thumbnails(plain: Arc<Vec<u8>>, stop: &AtomicBool) -> (Vec<String>, usize) {
    let Ok(r) = folitecta_core::Renderer::new(plain) else {
        return (Vec::new(), 0);
    };
    let sizes = r.page_sizes();
    let mut out = Vec::new();
    for (p, (w, _)) in sizes.iter().enumerate().take(THUMBS) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let scale = (144.0 / w.max(1.0)).clamp(0.05, 1.0);
        if let Ok(page) = r.render_page(p, scale) {
            out.push(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&page.png)
            ));
        }
    }
    let more = sizes.len().saturating_sub(out.len());
    (out, more)
}

/// Prepare a row (away from the interface thread); `stop` ends it early.
pub fn run_job(job: Job, stop: &AtomicBool) -> Result<Prepared, Problem> {
    match job {
        Job::File {
            path,
            same_as,
            protected,
        } => {
            let bytes =
                std::fs::read(&path).map_err(|e| Problem::Read(format!("{}: {e}", path.display())))?;
            let (facts, plain) = match Document::from_bytes(bytes.clone(), None, None) {
                Ok(d) => {
                    // FOL-L036: the saved file of a protected document is attached
                    // only when it is protected too.
                    if protected && !d.is_protected() {
                        return Err(Problem::SavedPlain);
                    }
                    (Some(d.copy_facts()), Some(d.bytes()))
                }
                Err(Error::PasswordRequired) => match same_as.map(|s| s.inspect()) {
                    Some(Ok((facts, plain))) => (Some(facts), Some(plain)),
                    _ => (None, None),
                },
                Err(e) => return Err(Problem::Read(e.to_string())),
            };
            let (thumbs, more_pages) = match plain {
                Some(p) => thumbnails(p, stop),
                None => (Vec::new(), 0),
            };
            Ok(Prepared {
                bytes: Arc::new(bytes),
                facts,
                thumbs,
                more_pages,
                text: None,
                made_here: false,
            })
        }
        Job::Copy { source, options } => {
            let protected = source.is_protected();
            let copy = source
                .prepare_until(&options, stop)
                .map_err(|e| core_problem(e, protected))?;
            let (thumbs, more_pages) = thumbnails(copy.plain.clone(), stop);
            Ok(Prepared {
                bytes: Arc::new(copy.bytes),
                facts: Some(copy.facts),
                thumbs,
                more_pages,
                text: None,
                made_here: true,
            })
        }
        Job::Data { bytes } => Ok(Prepared {
            text: Some(first_lines(&bytes, 40)),
            bytes: Arc::new(bytes),
            facts: None,
            thumbs: Vec::new(),
            more_pages: 0,
            made_here: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(title: &str, saved: Option<&str>, dirty: bool, has_form: bool) -> DocFacts {
        DocFacts {
            tab: 1,
            title: title.into(),
            saved: saved.map(PathBuf::from),
            dirty,
            pages: 5,
            current_page: 2,
            has_form,
            ..DocFacts::default()
        }
    }

    fn ready(bytes: &[u8]) -> Prep {
        Prep::Ready(Arc::new(Prepared {
            bytes: Arc::new(bytes.to_vec()),
            facts: None,
            thumbs: Vec::new(),
            more_pages: 0,
            text: None,
            made_here: false,
        }))
    }

    fn kinds(d: &MailDraft) -> Vec<(String, bool, String)> {
        d.items
            .iter()
            .map(|i| {
                let k = match &i.kind {
                    ItemKind::Saved(_) => "saved",
                    ItemKind::Current => "current",
                    ItemKind::Data(DataKind::Form(_)) => "form",
                    ItemKind::Data(DataKind::Comments(_)) => "comments",
                };
                (k.to_string(), i.include, i.name.clone())
            })
            .collect()
    }

    fn row(d: &MailDraft, s: (&str, bool, &str)) -> bool {
        kinds(d).contains(&(s.0.to_string(), s.1, s.2.to_string()))
    }

    #[test]
    fn drafts_start_with_what_each_command_names() {
        let a = facts("a.pdf", Some("/x/a.pdf"), false, true);
        let b = facts("b.pdf", Some("/x/b.pdf"), true, false);
        let mut c = facts("Untitled", None, true, false);
        c.comments = 2;
        let one = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: false,
            },
            std::slice::from_ref(&a),
        );
        assert_eq!(
            kinds(&one),
            [
                ("saved".to_string(), true, "a.pdf".to_string()),
                ("form".to_string(), false, "a-data.xfdf".to_string())
            ]
        );
        assert!(!one.zip);
        assert_eq!(one.subject, "a.pdf");
        let dirty = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: false,
            },
            std::slice::from_ref(&b),
        );
        assert!(row(&dirty, ("current", true, "b.pdf")) && row(&dirty, ("saved", false, "b (saved).pdf")));
        let all = new_draft(
            Lang::En,
            Start::Documents { all: true, zip: true },
            &[a.clone(), b, c],
        );
        let includes: Vec<bool> = all.items.iter().map(|i| i.include).collect();
        assert_eq!(includes, [true, false, true, false, true, false]);
        assert_eq!(all.zip_name, "Documents.zip");
        assert_eq!(zip_file_name(&all), "Documents.zip");
        assert!(all.zip);
        assert_eq!(all.subject, "a.pdf, b.pdf, Untitled.pdf");
        let data = new_draft(Lang::En, Start::FormData, &[a]);
        assert_eq!(
            kinds(&data)[0],
            ("form".to_string(), true, "a-data.xfdf".to_string())
        );
        assert_eq!(kinds(&data)[1], ("saved".to_string(), false, "a.pdf".to_string()));
        assert_eq!(data.subject, "a-data.xfdf");
    }

    #[test]
    fn options_become_a_plan() {
        let mut it = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: false,
            },
            &[facts("u", None, true, false)],
        )
        .items
        .remove(0);
        assert_eq!(it.kind, ItemKind::Current);
        it.pages = Pages::Range;
        it.range = "3-1".into();
        assert_eq!(
            it.plan().unwrap(),
            Plan::Copy(CopyOptions {
                pages: Some(vec![0, 1, 2]),
                comments: CopyComments::Keep
            })
        );
        it.range = "9".into();
        assert_eq!(it.plan(), Err(Problem::Range(RangeError::OutOfBounds(5))));
        it.range = "x".into();
        assert_eq!(it.plan(), Err(Problem::Range(RangeError::NotARange("x".into()))));
        it.range = String::new();
        assert_eq!(it.plan().unwrap(), Plan::Copy(CopyOptions::default()));
        it.pages = Pages::Current;
        assert!(matches!(it.plan().unwrap(), Plan::Copy(o) if o.pages == Some(vec![2])));
        // The saved file of a document with changes goes as it is.
        let mut saved = it.clone();
        saved.kind = ItemKind::Saved("/x/u.pdf".into());
        saved.pages = Pages::Range;
        saved.range = "1".into();
        assert_eq!(saved.plan().unwrap(), Plan::File("/x/u.pdf".into()));
        // Without changes it can be cut, and goes as it is with the defaults.
        saved.doc.dirty = false;
        saved.pages = Pages::All;
        saved.comments = CopyComments::Remove;
        assert_eq!(
            saved.plan().unwrap(),
            Plan::Copy(CopyOptions {
                pages: None,
                comments: CopyComments::Remove
            })
        );
        saved.comments = CopyComments::Keep;
        assert_eq!(saved.plan().unwrap(), Plan::File("/x/u.pdf".into()));
        // Data of a protected document is refused (FOL-L036).
        let mut data = it.clone();
        data.kind = ItemKind::Data(DataKind::Form(FormDataFormat::Csv));
        data.doc.has_form = true;
        assert_eq!(
            data.plan().unwrap(),
            Plan::Data(DataKind::Form(FormDataFormat::Csv))
        );
        data.doc.protected = true;
        assert_eq!(data.plan(), Err(Problem::DataOfProtected));
    }

    #[test]
    fn names_are_safe_unique_and_keep_their_extension() {
        let base = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: false,
            },
            &[facts("a.pdf", None, true, false)],
        )
        .items
        .remove(0);
        let mut items = Vec::new();
        for (name, kind) in [
            ("a/b.pdf", ItemKind::Current),
            ("A.PDF", ItemKind::Current),
            ("a.pdf", ItemKind::Current),
            ("data", ItemKind::Data(DataKind::Form(FormDataFormat::Csv))),
            ("left out", ItemKind::Current),
        ] {
            let mut it = base.clone();
            it.name = name.into();
            it.kind = kind;
            items.push(it);
        }
        items[4].include = false;
        assert_eq!(
            final_names(&items),
            [
                Some("a_b.pdf".to_string()),
                Some("A.PDF".to_string()),
                Some("a (2).pdf".to_string()),
                Some("data.csv".to_string()),
                None
            ]
        );
        let mut d = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: true,
            },
            &[facts("a.pdf", None, true, false)],
        );
        d.zip_name = "x.ZIP".into();
        assert_eq!(zip_file_name(&d), "x.ZIP");
        d.zip_name = "x".into();
        assert_eq!(zip_file_name(&d), "x.zip");
    }

    #[test]
    fn attachments_are_the_prepared_bytes_or_one_archive() {
        let mut d = new_draft(
            Lang::En,
            Start::Documents {
                all: true,
                zip: false,
            },
            &[
                facts("x.pdf", None, true, false),
                facts("x.pdf", None, true, false),
            ],
        );
        d.items[0].prep = ready(b"%PDF-first");
        d.items[1].prep = ready(b"%PDF-second");
        let files = attachments(&d).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["x.pdf", "x (2).pdf"]);
        assert_eq!(files[1].bytes.as_slice(), b"%PDF-second");
        let parts: Vec<(String, &[u8])> = files
            .iter()
            .map(|f| (f.name.clone(), f.bytes.as_slice()))
            .collect();
        let eml = folitecta_core::email::write_eml_draft(&d.subject, "", &parts);
        let back = folitecta_core::email::read_email(&eml).unwrap();
        assert_eq!(back.attachments.len(), 2);
        assert_eq!(back.attachments[0], ("x.pdf".to_string(), b"%PDF-first".to_vec()));
        d.zip = true;
        d.zip_name = "x".into();
        assert!(attachments(&d).is_none());
        assert!(zip_wanted(&d));
        let zip = zip_of(&prepared_rows(&d).unwrap()).unwrap();
        d.zip_prep = ready(&zip);
        let files = attachments(&d).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "x.zip");
        let entries = folitecta_core::zip::read(&files[0].bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].name, "x (2).pdf");
        assert_eq!(entries[1].data, b"%PDF-second");
        // Two entries of one name are numbered by the archive too.
        let same = [
            Outgoing {
                name: "a.pdf".into(),
                bytes: Arc::new(b"1".to_vec()),
                made_here: false,
            },
            Outgoing {
                name: "A.pdf".into(),
                bytes: Arc::new(b"2".to_vec()),
                made_here: false,
            },
        ];
        let names: Vec<String> = folitecta_core::zip::read(&zip_of(&same).unwrap())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["a.pdf", "A (2).pdf"]);
    }

    #[test]
    fn only_ready_attachments_can_go() {
        let mut d = new_draft(
            Lang::En,
            Start::Documents {
                all: false,
                zip: false,
            },
            &[facts("x.pdf", None, true, false)],
        );
        assert!(attachments(&d).is_none());
        d.items[0].prep = Prep::Running(0);
        assert!(attachments(&d).is_none());
        d.items[0].prep = Prep::Failed(Problem::Closed);
        assert!(attachments(&d).is_none());
        d.items[0].prep = ready(b"%PDF");
        assert!(attachments(&d).is_some());
        d.items[0].include = false;
        assert!(attachments(&d).is_none());
        d.items[0].include = true;
        d.zip = true;
        assert!(attachments(&d).is_none());
    }

    #[test]
    fn rows_are_started_once_and_two_at_a_time() {
        let docs: Vec<DocFacts> = (0..4)
            .map(|i| facts(&format!("d{i}.pdf"), None, true, false))
            .collect();
        let mut d = new_draft(
            Lang::En,
            Start::Documents {
                all: true,
                zip: false,
            },
            &docs,
        );
        d.selected = 2;
        assert_eq!(rows_to_start(&d, 2), [2, 0]);
        d.items[2].prep = Prep::Running(0);
        d.items[0].prep = Prep::Running(0);
        // Typing the subject or the message changes no row: nothing starts.
        d.subject = "typed".into();
        d.subject_auto = false;
        d.body = "hello".into();
        assert!(rows_to_start(&d, 2).is_empty());
        // A row changed while it runs waits for a free place, then starts again.
        touch(&mut d, 2);
        assert_eq!(d.items[2].rev, 1);
        assert_eq!(rows_to_start(&d, 2), [2]);
        assert_eq!(d.subject, "typed");
        d.items[2].prep = ready(b"%PDF");
        d.items[0].prep = ready(b"%PDF");
        assert_eq!(rows_to_start(&d, 2), [1, 3]);
    }

    #[test]
    fn jobs_prepare_copies_data_and_files() {
        let stop = AtomicBool::new(false);
        let doc = Document::new_blank(6, 200.0, 300.0).unwrap();
        let p = run_job(
            Job::Copy {
                source: doc.copy_source(),
                options: CopyOptions::default(),
            },
            &stop,
        )
        .unwrap();
        assert_eq!(p.facts.as_ref().unwrap().pages, 6);
        assert_eq!(p.thumbs.len(), THUMBS);
        assert_eq!(p.more_pages, 2);
        assert!(p.thumbs.iter().all(|t| t.starts_with("data:image/png;base64,")));
        assert!(p.made_here);
        let xfdf = doc.export_form_data(FormDataFormat::Xfdf);
        let p = run_job(Job::Data { bytes: xfdf }, &stop).unwrap();
        assert!(p.text.unwrap().starts_with("<?xml"));
        let dir = std::env::temp_dir().join(format!("folitecta-maildraft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.pdf");
        let saved = [
            doc.bytes().as_slice(),
            b"\ntrailing data after the end".as_slice(),
        ]
        .concat();
        std::fs::write(&file, &saved).unwrap();
        let p = run_job(
            Job::File {
                path: file.clone(),
                same_as: None,
                protected: false,
            },
            &stop,
        )
        .unwrap();
        assert_eq!(p.bytes.as_slice(), saved.as_slice());
        assert!(!p.made_here);
        // The saved file of a protected document must be protected (FOL-L036).
        assert_eq!(
            run_job(
                Job::File {
                    path: file.clone(),
                    same_as: None,
                    protected: true,
                },
                &stop,
            ),
            Err(Problem::SavedPlain)
        );
        // A stopped preparation ends early.
        stop.store(true, Ordering::Relaxed);
        assert!(
            run_job(
                Job::Copy {
                    source: doc.copy_source(),
                    options: CopyOptions::default(),
                },
                &stop,
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
