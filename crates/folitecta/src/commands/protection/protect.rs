//! Protect: redaction, password protection, document scripts.

use std::path::PathBuf;

use folitecta_core::redaction::{
    CheckProblem, PictureReason, QuotePlace, RedactionCheck, RedactionOptions, RedactionReport,
    check_redacted_bytes,
};

use crate::commands::{doc_dialog, handlers, no_doc};
use crate::files;
use dioxus::prelude::*;

use crate::i18n::{Lang, t, tf};
use crate::state::{AppState, Dialog, DocTab, RedactionPending, ToastKind, Tool};

/// Run `f` on the tab `id` (when it is still open).
fn with_tab<T>(st: AppState, id: u64, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
    let mut docs = st.docs_mut();
    let tab = docs.iter_mut().find(|t| t.id == id)?;
    let out = f(tab);
    tab.refresh();
    Some(out)
}

fn reason_key(r: PictureReason) -> Option<&'static str> {
    Some(match r {
        PictureReason::Chosen => return None,
        PictureReason::Font => "toast.redaction_reason.font",
        PictureReason::Shading => "toast.redaction_reason.shading",
        PictureReason::Pattern => "toast.redaction_reason.pattern",
        PictureReason::SoftMask => "toast.redaction_reason.softmask",
        PictureReason::Nesting => "toast.redaction_reason.nesting",
        PictureReason::Content => "toast.redaction_reason.content",
        PictureReason::Picture => "toast.redaction_reason.picture",
        PictureReason::Check => "toast.redaction_reason.check",
    })
}

/// What a redaction did, as the toasts tell it.
fn report_toasts(st: AppState, r: &RedactionReport) {
    let lang = st.lang();
    let mut detail = tf(
        lang,
        "toast.redaction_detail",
        &[
            ("glyphs", &r.glyphs.to_string()),
            ("pictures", &r.pictures.to_string()),
            ("paths", &r.paths.to_string()),
            ("comments", &r.comments.to_string()),
            ("fields", &r.fields.to_string()),
        ],
    );
    // Pages drawn as pictures, by reason.
    let mut reasons: Vec<PictureReason> = r.reasons.iter().map(|(_, x)| *x).collect();
    reasons.dedup();
    for reason in reasons {
        let pages: Vec<usize> = r
            .reasons
            .iter()
            .filter(|(_, x)| *x == reason)
            .map(|(p, _)| *p)
            .collect();
        let pages = crate::util::format_pages(&pages);
        detail.push(' ');
        detail.push_str(&match reason_key(reason) {
            None => tf(lang, "toast.redaction_as_pictures", &[("pages", &pages)]),
            Some(key) => tf(
                lang,
                "toast.redaction_fallback",
                &[("reason", &t(lang, key)), ("pages", &pages)],
            ),
        });
    }
    let s = &r.sanitized;
    let mut parts: Vec<String> = Vec::new();
    let count = |key: &str, n: usize| tf(lang, key, &[("count", &n.to_string())]);
    if r.signatures > 0 {
        parts.push(count("toast.redaction_part.signatures", r.signatures));
    }
    if s.metadata {
        parts.push(t(lang, "toast.redaction_part.metadata"));
    }
    if s.attachments > 0 {
        parts.push(count("toast.redaction_part.attachments", s.attachments));
    }
    if s.hidden > 0 {
        parts.push(count("toast.redaction_part.hidden", s.hidden));
    }
    if s.hidden_layers > 0 {
        parts.push(count("toast.redaction_part.layers", s.hidden_layers));
    }
    if s.quotes > 0 {
        parts.push(count("toast.redaction_part.quotes", s.quotes));
    }
    if s.scripts > 0 {
        parts.push(count("toast.redaction_part.scripts", s.scripts));
    }
    if !parts.is_empty() {
        detail.push(' ');
        detail.push_str(&tf(
            lang,
            "toast.redaction_sanitized",
            &[("list", &parts.join(", "))],
        ));
    }
    if !s.failed.is_empty() {
        let names: Vec<String> = s
            .failed
            .iter()
            .map(|p| {
                let key = match p {
                    folitecta_core::redaction::SanitizePart::Metadata => "redact.sanitize.metadata",
                    folitecta_core::redaction::SanitizePart::Attachments => "redact.sanitize.attachments",
                    folitecta_core::redaction::SanitizePart::Hidden => "redact.sanitize.hidden",
                    folitecta_core::redaction::SanitizePart::Quoting => "redact.sanitize.quoting",
                    folitecta_core::redaction::SanitizePart::Scripts => "redact.sanitize.scripts",
                };
                t(lang, key)
            })
            .collect();
        st.toast(
            ToastKind::Warn,
            tf(
                lang,
                "toast.redaction_part_failed",
                &[("list", &names.join(", "))],
            ),
            "",
        );
    }
    if s.conformance_kept {
        detail.push(' ');
        detail.push_str(&t(lang, "toast.redaction_conformance"));
    }
    st.toast(
        ToastKind::Ok,
        tf(
            lang,
            "toast.redactions_applied",
            &[("count", &r.areas.to_string())],
        ),
        detail,
    );
    if r.blurred_areas > 0 {
        st.toast(
            ToastKind::Warn,
            tf(
                lang,
                "toast.redaction_blurred",
                &[("count", &r.blurred_areas.to_string())],
            ),
            "",
        );
    }
}

/// Apply the redaction marks of `pages` (all when `None`) as `opts` says, on
/// a copy of the document taken now, away from the interface; the result is
/// adopted as one undo step when the document did not change meanwhile.
/// With `save_copy`, the result is then saved as a new file, checked first.
pub fn apply_redactions_now(
    st: AppState,
    pages: Option<Vec<usize>>,
    opts: RedactionOptions,
    save_copy: bool,
) {
    let Some((id, snapshot, busy)) = st.read_active(|t| (t.id, t.doc.snapshot(), t.redacting)) else {
        return;
    };
    let lang = st.lang();
    if busy {
        st.toast(ToastKind::Info, t(lang, "toast.redaction_busy"), "");
        return;
    }
    with_tab(st, id, |t| t.redacting = true);
    st.set_busy(true);
    st.toast(ToastKind::Info, t(lang, "toast.redaction_working"), "");
    let revision = snapshot.revision();
    dioxus::core::spawn_forever(async move {
        let worked = tokio::task::spawn_blocking(move || {
            let mut doc = snapshot.open()?;
            let report = doc.apply_redactions_opts(pages.as_deref(), &opts)?;
            Ok::<_, folitecta_core::Error>((doc, report))
        })
        .await;
        st.set_busy(false);
        let adopted = with_tab(st, id, |t| {
            t.redacting = false;
            let (doc, report) = match worked {
                Ok(Ok(w)) => w,
                Ok(Err(e)) => return Err(e.to_string()),
                Err(e) => return Err(e.to_string()),
            };
            if t.doc.revision() != revision {
                return Ok(None);
            }
            t.doc.adopt_state(doc).map_err(|e| e.to_string())?;
            t.redaction = Some(RedactionPending {
                revision: t.doc.revision(),
                report: report.clone(),
            });
            Ok(Some(report))
        });
        match adopted {
            // The tab was closed meanwhile: nothing to adopt.
            None => {}
            Some(Err(e)) => st.error(e),
            Some(Ok(None)) => st.toast(ToastKind::Warn, t(st.lang(), "toast.redaction_changed"), ""),
            Some(Ok(Some(report))) => {
                files::set_window_title(st);
                st.set_dialog(Dialog::None);
                report_toasts(st, &report);
                if save_copy {
                    save_redacted_copy(st, id);
                }
            }
        }
    });
}

/// A problem of the check, as the toasts tell it.
pub fn problem_text(lang: Lang, p: &CheckProblem) -> String {
    let page = |n: usize| (n + 1).to_string();
    match p {
        CheckProblem::Text { page: n } => tf(lang, "toast.redaction_problem.text", &[("page", &page(*n))]),
        CheckProblem::Picture { page: n } => {
            tf(lang, "toast.redaction_problem.picture", &[("page", &page(*n))])
        }
        CheckProblem::Drawing { page: n } => {
            tf(lang, "toast.redaction_problem.drawing", &[("page", &page(*n))])
        }
        CheckProblem::Residual { page: Some(n) } => {
            tf(lang, "toast.redaction_problem.residual", &[("page", &page(*n))])
        }
        CheckProblem::Residual { page: None } => t(lang, "toast.redaction_problem.residual_doc"),
        CheckProblem::Revisions => t(lang, "toast.redaction_problem.revisions"),
        CheckProblem::Quote { place } => {
            let key = match place {
                QuotePlace::Bookmarks => "toast.redaction_place.bookmarks",
                QuotePlace::Metadata => "toast.redaction_place.metadata",
                QuotePlace::Names => "toast.redaction_place.names",
                QuotePlace::Links => "toast.redaction_place.links",
                QuotePlace::Tags => "toast.redaction_place.tags",
            };
            tf(lang, "toast.redaction_problem.quote", &[("place", &t(lang, key))])
        }
    }
}

/// Ask where to save the redacted document `tab` as a new file, then save
/// it through the check.
pub fn save_redacted_copy(st: AppState, tab: u64) {
    let name = files::suggested_name(st, "-redacted");
    dioxus::core::spawn_forever(async move {
        let file = files::pdf_dialog()
            .set_title("Save the Redacted Copy")
            .set_file_name(&name)
            .save_file()
            .await;
        match file {
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.redaction_not_saved"), ""),
            Some(f) => {
                let _ = save_redacted_copy_to(st, tab, f.path().to_path_buf()).await;
            }
        }
    });
}

/// Save the redacted document `tab` to `path`: the exact bytes to write are
/// checked first (read again from scratch, decrypted with the document's
/// key when it is protected) and written, atomically, only when the check
/// passes. The document then lives at `path`.
pub async fn save_redacted_copy_to(st: AppState, tab: u64, path: PathBuf) -> Result<RedactionCheck, String> {
    let lang = st.lang();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some((snapshot, report)) = st
        .docs
        .read()
        .iter()
        .find(|t| t.id == tab)
        .map(|t| (t.doc.snapshot(), t.redaction.as_ref().map(|r| r.report.clone())))
    else {
        return Err(t(lang, "toast.no_document"));
    };
    let report = report.unwrap_or_default();
    st.set_busy(true);
    let target = path.clone();
    let done = tokio::task::spawn_blocking(move || {
        let mut doc = snapshot.open().map_err(|e| e.to_string())?;
        let prepared = doc.prepare_save().map_err(|e| e.to_string())?;
        let check = check_redacted_bytes(&prepared.bytes, prepared.key.as_ref(), &report)
            .map_err(|e| e.to_string())?;
        if check.passed() {
            folitecta_core::document::write_pdf_file(&target, &prepared.bytes).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>((check, prepared))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    st.set_busy(false);
    let (check, prepared) = match done {
        Ok(x) => x,
        Err(e) => {
            st.error(&e);
            return Err(e);
        }
    };
    if !check.passed() {
        let problems: Vec<String> = check.problems.iter().map(|p| problem_text(lang, p)).collect();
        st.toast(
            ToastKind::Error,
            tf(
                lang,
                "toast.redaction_check_failed",
                &[("name", &name), ("count", &check.problems.len().to_string())],
            ),
            problems.join("; "),
        );
        return Ok(check);
    }
    with_tab(st, tab, |t| {
        t.doc.finish_save(&path, &prepared);
        t.redaction = None;
    });
    st.ui_mut().push_recent(&path);
    st.save_prefs();
    files::set_window_title(st);
    crate::recovery::checkpoint(st);
    let mut detail = Vec::new();
    if check.quoted_elsewhere > 0 {
        detail.push(tf(
            lang,
            "toast.redaction_quoted",
            &[("count", &check.quoted_elsewhere.to_string())],
        ));
    }
    if check.visual_only > 0 {
        detail.push(tf(
            lang,
            "toast.redaction_visual",
            &[("count", &check.visual_only.to_string())],
        ));
    }
    if check.unchecked_pictures > 0 {
        detail.push(tf(
            lang,
            "toast.redaction_unchecked",
            &[("count", &check.unchecked_pictures.to_string())],
        ));
    }
    st.toast(
        ToastKind::Ok,
        tf(lang, "toast.redaction_checked", &[("name", &name)]),
        detail.join(" "),
    );
    Ok(check)
}

/// The active tab has a redaction applied whose saved bytes are not checked
/// yet, and nothing changed since: saves go through the check.
pub fn unchecked_redaction(st: AppState) -> Option<u64> {
    st.read_active(|t| {
        t.redaction
            .as_ref()
            .filter(|r| r.revision == t.doc.revision())
            .map(|_| t.id)
    })
    .flatten()
}

fn apply_redactions(st: AppState, current_page: bool) {
    if no_doc(st) {
        return;
    }
    let (page, pending) = st
        .read_active(|t| {
            let all = t.doc.pending_redactions();
            let n = if current_page {
                all.iter().filter(|(p, _)| *p == t.page).count()
            } else {
                all.len()
            };
            (t.page, n)
        })
        .unwrap();
    if pending == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_redactions"), "");
        return;
    }
    st.set_dialog(Dialog::ApplyRedactions {
        page: current_page.then_some(page),
    });
}

handlers! {
    "tool.annot.redaction" => |st| { if !no_doc(st) { st.set_tool(Tool::Redact) } },
    "document.applyRedactions" => |st| apply_redactions(st, false),
    "annots.applyRedactions" => |st| apply_redactions(st, true),
    "document.security" => |st| doc_dialog(st, Dialog::Security),
    "standardSecPolicy" => |st| doc_dialog(st, Dialog::Security),
    "saveUnrestricted" => |st| files::save_unprotected_copy(st),
}
