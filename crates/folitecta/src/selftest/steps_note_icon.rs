//! Interaction step 51: the icon of a note — Comments › Note Icon on the
//! selected note shows the icons, a click gives the note the one clicked;
//! with no note selected it chooses the icon of the new notes.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 51. A note on the last page, selected.
    let page = st
        .read_active(|t| t.doc.page_count().saturating_sub(1))
        .unwrap_or(0);
    let made = st
        .with_active(|t| {
            let r = t
                .doc
                .add_note(page, 300.0, 300.0, "Icon test", "Reviewer", [1.0, 0.85, 0.2]);
            t.refresh();
            let index = t.doc.annotations(page).ok()?.len().checked_sub(1)?;
            r.ok()?;
            t.selected_annot = Some((page, index));
            Some(index)
        })
        .flatten();
    let Some(index) = made else {
        cx.record("note icons", false, "the note could not be made".into());
        return;
    };
    crate::commands::dispatch(st, "noteIcon");
    pause(500).await;
    let shown = eval("return document.querySelectorAll('.dialog .note-icons .choice').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-note-icon.png"));
    // The star is the last icon.
    let _ = eval("[...document.querySelectorAll('.dialog .note-icons .choice')].pop()?.click();").await;
    pause(500).await;
    let icon = st.read_active(|t| t.doc.note_icon_of(page, index)).flatten();
    let closed = matches!((st.dialog)(), Dialog::None);
    // No note selected: the icon of the new notes.
    st.with_active(|t| t.selected_annot = None);
    crate::commands::dispatch(st, "noteIcon");
    pause(400).await;
    let _ = eval("[...document.querySelectorAll('.dialog .note-icons .choice')][2]?.click();").await;
    pause(400).await;
    let default_icon = st.value("note.icon");
    st.ui_mut().values.remove("note.icon");
    cx.record(
        "note icons: the icon of the selected note chosen from a grid, and the icon of new notes",
        shown == folitecta_core::NOTE_ICONS.len() as u64
            && icon.as_deref() == Some("Star")
            && closed
            && default_icon == "Key",
        format!("shown {shown}, icon {icon:?}, closed {closed}, new notes {default_icon:?}"),
    );
}
