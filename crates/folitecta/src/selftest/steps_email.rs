//! Interaction step: an e-mail message (`.eml`) opened like a PDF becomes a
//! new document with its header lines and text on the page and its attached
//! file embedded; a draft written for the mail program carries the files.

use crate::state::AfterUnsaved;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
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
    // The draft of a message with a file attached, as the mail commands write it.
    let file = dir.join("draft-attachment.pdf");
    let _ = std::fs::write(&file, b"%PDF-1.7 draft test");
    let draft = crate::mail::write_draft("Draft test", "", std::slice::from_ref(&file))
        .ok()
        .and_then(|p| std::fs::read(p).ok())
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
