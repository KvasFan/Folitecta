//! Interaction step: an e-mail message (`.eml`) opened like a PDF becomes a
//! new document with its header lines and text on the page and its attached
//! file embedded; the e-mail dialog prepares two pages of a protected
//! document without its comments, reports a mail program that cannot take
//! the files, and saves the draft, which holds that copy still protected.
//! A stand-in takes the place of the mail program: nothing is started.

use std::path::PathBuf;
use std::sync::Arc;

use folitecta_core::{CopyComments, Document, Error, SecurityPermissions, TextLayout};

use crate::commands::mail::{MAIL, Pages, Phase, Prep};
use crate::mail::{HandOverError, Outgoing, Sent};
use crate::state::AfterUnsaved;

use super::*;

/// The mail program of the step: it never takes the files.
fn no_mail_program(_: &str, _: &str, _: &[PathBuf]) -> Result<Sent, HandOverError> {
    Err(HandOverError::NoProgram("self-test".into()))
}

async fn opens_as_document(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    let note = b"Note of the meeting".to_vec();
    let eml = folitecta_core::email::write_eml_draft(
        "Meeting notes",
        "Hello,\nthe notes are attached.",
        &[("notes.txt".to_string(), note.clone())],
    );
    let path = dir.join("meeting.eml");
    let written = std::fs::write(&path, &eml).is_ok();
    let before = st.docs.read().len();
    crate::files::open_path(st, path.clone(), None);
    pause(600).await;
    let after = st.docs.read().len();
    let opened = st
        .read_active(|t| {
            let atts = t.doc.attachments();
            let data = atts.first().and_then(|a| t.doc.attachment_data(a).ok());
            (
                t.title(),
                t.doc.page_text(0),
                atts.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
                data,
                t.id,
            )
        })
        .unwrap_or_default();
    let (title, text, names, data, id) = opened;
    let _ = capture_window(&dir.join("interaction-email.png"));
    if after > before {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    // A draft with a file attached, written the way the dialog writes it.
    let draft_path = dir.join("draft-test.eml");
    let file = Outgoing {
        name: "draft-attachment.pdf".into(),
        bytes: Arc::new(b"%PDF-1.7 draft test".to_vec()),
        made_here: false,
    };
    let draft = crate::mail::write_draft(&draft_path, "Draft test", "", &[file])
        .ok()
        .and_then(|()| std::fs::read(&draft_path).ok())
        .and_then(|d| folitecta_core::email::read_email(&d).ok())
        .map(|m| m.attachments)
        .unwrap_or_default();
    cx.record(
        "an e-mail message opens as a document with its text and attachment, and drafts carry their files",
        written
            && after == before + 1
            && title == "meeting.pdf"
            && text.contains("Meeting notes")
            && text.contains("the notes are attached")
            && names == ["notes.txt"]
            && data.as_deref() == Some(note.as_slice())
            && draft.len() == 1
            && draft[0].0 == "draft-attachment.pdf",
        format!(
            "tabs {before} -> {after}, title {title:?}, attachments {names:?}, text has subject {}, draft files {}",
            text.contains("Meeting notes"),
            draft.len()
        ),
    );
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    opens_as_document(cx).await;
    let (st, dir) = (cx.st, cx.dir);
    let lang = st.lang();
    let launches = crate::mail::launches();
    crate::mail::set_fake(Some(no_mail_program));
    let sample = st.read_active(|t| t.id);

    // A protected document of three pages with a note on the second.
    let path = dir.join("mail-protected.pdf");
    let made = Document::from_text(
        "MAIL-PAGE-ONE\u{c}MAIL-PAGE-TWO\u{c}MAIL-PAGE-THREE",
        TextLayout::default(),
    )
    .and_then(|mut d| {
        d.add_note(1, 40.0, 500.0, "MAIL-NOTE", "Reviewer", [1.0, 0.8, 0.2])?;
        d.save_encrypted(
            &path,
            "user-pw",
            "owner-pw",
            SecurityPermissions::default(),
            folitecta_core::EncryptionProfile::Aes128,
        )
    });
    crate::files::open_path(st, path.clone(), Some("user-pw"));
    pause(600).await;
    let tab = st
        .read_active(|t| (t.title() == "mail-protected.pdf").then_some(t.id))
        .flatten();

    // The dialog, driven through the page: pages 1-2, comments removed.
    crate::commands::dispatch(st, "sendByEmail");
    pause(500).await;
    let opened = matches!((st.dialog)(), Dialog::Mail);
    let _ = eval("document.querySelector('.dialog .mail-pages-range')?.click();").await;
    pause(400).await;
    let _ = eval(
        "const i = document.querySelector('.dialog .mail-range'); if (i) { i.value = '1-2'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = eval(
        "const s = document.querySelector('.dialog .mail-comments'); if (s) { s.value = 'remove'; s.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    // Prepared away from the interface thread (at most 20 s).
    let mut prepared = false;
    for _ in 0..100 {
        pause(200).await;
        prepared = MAIL.peek().as_ref().is_some_and(|d| {
            d.items.first().is_some_and(|it| {
                it.pages == Pages::Range
                    && it.range == "1-2"
                    && it.comments == CopyComments::Remove
                    && matches!(it.prep, Prep::Ready(_))
            })
        });
        if prepared {
            break;
        }
    }
    pause(300).await;
    let facts = eval("return document.querySelector('.dialog .mail-facts')?.innerText ?? '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let pages_fact = crate::i18n::tf(lang, "dialog.mail.facts_pages", &[("count", "2")]);
    let protected_fact = crate::i18n::t(lang, "dialog.mail.protected");
    let _ = capture_window(&dir.join("interaction-email-dialog.png"));

    // Open in Mail Program: the stand-in has no mail program; the dialog says
    // so and offers the draft and the files.
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut failed = false;
    for _ in 0..50 {
        pause(200).await;
        failed = MAIL
            .peek()
            .as_ref()
            .is_some_and(|d| matches!(d.phase, Phase::Failed(HandOverError::NoProgram(_))));
        if failed {
            break;
        }
    }
    pause(300).await;
    let offered = eval(
        "return ['.mail-failed', '.mail-save-again', '.mail-open-draft', '.mail-show-files'].every(s => document.querySelector('.dialog ' + s));",
    )
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    let outbox = MAIL.peek().as_ref().and_then(|d| d.outbox.clone());
    let _ = capture_window(&dir.join("interaction-email-failed.png"));

    // Save Draft…: the function behind the button, once the file is chosen
    // (the system's file dialog cannot be driven).
    let draft_path = dir.join("mail-draft.eml");
    let saved = crate::commands::mail::save_draft_to(st, &draft_path).await;
    pause(300).await;
    let closed = matches!((st.dialog)(), Dialog::None) && MAIL.peek().is_none();
    let draft = std::fs::read(&draft_path).unwrap_or_default();
    let unsent = draft.starts_with(b"X-Unsent: 1");
    let message = folitecta_core::email::read_email(&draft).ok();
    let (subject, attachments) = message.map(|m| (m.subject, m.attachments)).unwrap_or_default();
    let attachment = attachments.first().map(|a| a.1.clone()).unwrap_or_default();
    let locked = matches!(
        Document::from_bytes(attachment.clone(), None, None),
        Err(Error::PasswordRequired)
    );
    let copy = Document::from_bytes(attachment, None, Some("user-pw")).ok();
    let (pages, comments, first, third_gone) = copy
        .map(|d| {
            (
                d.page_count(),
                d.comments().len(),
                d.page_text(0).contains("MAIL-PAGE-ONE"),
                !d.holds(b"MAIL-PAGE-THREE") && !d.holds(b"MAIL-NOTE"),
            )
        })
        .unwrap_or_default();
    let started = crate::mail::launches() - launches;
    crate::mail::set_fake(None);
    if let Some(d) = outbox {
        let _ = std::fs::remove_dir_all(d);
    }
    // The protected document is closed; the sample is the active one again.
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    let back = st.read_active(|t| t.id) == sample;
    cx.record(
        "the e-mail dialog prepares pages 1-2 of a protected document without comments, reports a missing mail program and saves the draft, still protected, without starting any program",
        made.is_ok()
            && tab.is_some()
            && opened
            && prepared
            && facts.contains(&pages_fact)
            && facts.contains(&protected_fact)
            && failed
            && offered
            && saved.is_ok()
            && closed
            && unsent
            && attachments.len() == 1
            && attachments[0].0 == "mail-protected.pdf"
            && subject == "mail-protected.pdf"
            && locked
            && pages == 2
            && comments == 0
            && first
            && third_gone
            && started == 0
            && back,
        format!(
            "made {}, opened {opened}, prepared {prepared}, facts {facts:?}, failure reported {failed} with its buttons {offered}, saved {saved:?}, closed {closed}, unsent {unsent}, attachments {:?}, subject {subject:?}, needs the password {locked}, pages {pages}, comments {comments}, page 3 and the note gone {third_gone}, programs started by the mail module {started}, sample active again {back}",
            made.is_ok(),
            attachments.iter().map(|a| a.0.clone()).collect::<Vec<_>>()
        ),
    );
}
