//! Interaction step: text beyond WinAnsi in generated text. Typewriter text
//! typed in Polish, Greek and Cyrillic is drawn with an embedded FreeSans
//! subset; text with characters the font lacks is named while it is typed,
//! refused on OK with a translated message, and nothing changes (in the text
//! box dialog and in the dialog that edits the text of a comment); an import
//! whose comments cannot all be drawn names them and imports the others on
//! request. Tables and calculator tapes typed beyond WinAnsi are drawn with
//! the subset too; a table the font cannot draw is named in its editor and
//! refused on OK, which keeps the editor open with what was typed.

use super::*;
use crate::state::ToastKind;

/// Type `value` into the text area of the open dialog.
async fn type_text(value: &str) {
    let js = format!(
        "const a = document.querySelector('.dialog textarea'); if (a) {{ a.value = {}; a.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(value).unwrap_or_default()
    );
    let _ = eval(&js).await;
    pause(400).await;
}

async fn hint() -> String {
    eval("return document.querySelector('.dialog .glyph-hint')?.textContent || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn press_ok() {
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
}

fn error_toast_with(st: AppState, needle: &str) -> bool {
    st.toasts
        .read()
        .iter()
        .any(|t| t.kind == ToastKind::Error && t.message.contains(needle))
}

fn revision(st: AppState) -> u64 {
    st.read_active(|t| t.doc.revision()).unwrap_or(0)
}

/// Click with the typewriter tool at `at` (page display points): its dialog opens.
async fn typewriter_at(cx: &Ctx<'_>, at: (f32, f32)) -> bool {
    crate::commands::dispatch(cx.st, "tool.annot.typeWriter");
    pause(400).await;
    pointer_path(0, cx.z, &[at, at], false).await;
    pause(700).await;
    matches!((cx.st.dialog)(), Dialog::TextBox { border: false, .. })
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    st.go_to_page(0);
    pause(400).await;

    // Text beyond WinAnsi: drawn with an embedded subset of FreeSans.
    let text = "Łódź — Ελληνικά — Кириллица";
    let before = annots_on(st, 0).len();
    let opened = typewriter_at(cx, (100.0, 450.0)).await;
    type_text(text).await;
    let warned = hint().await;
    press_ok().await;
    let after = annots_on(st, 0);
    let added = after.len() == before + 1;
    let contents = after.last().map(|a| a.contents.clone()).unwrap_or_default();
    let index = after.len().saturating_sub(1);
    let fonts = st
        .read_active(|t| t.doc.annotation_fonts(0, index).unwrap_or_default())
        .unwrap_or_default();
    let embedded = fonts.len() == 1
        && fonts[0].subtype == "Type0"
        && fonts[0].embedded
        && fonts[0].to_unicode
        && fonts[0].base_font.ends_with("+FreeSans");
    cx.record(
        "text beyond WinAnsi is drawn with an embedded font subset",
        opened && warned.is_empty() && added && contents == text && embedded,
        format!("opened {opened}, hint {warned:?}, added {added}, contents {contents:?}, fonts {fonts:?}"),
    );

    // Characters the font lacks: named while typing, refused, nothing changed.
    let (rev, count) = (revision(st), annots_on(st, 0).len());
    let opened = typewriter_at(cx, (100.0, 520.0)).await;
    type_text("漢字").await;
    let warned = hint().await;
    press_ok().await;
    let still_open = matches!((st.dialog)(), Dialog::TextBox { .. });
    let toast = error_toast_with(st, "漢");
    let unchanged = revision(st) == rev && annots_on(st, 0).len() == count;
    let _ = capture_window(&cx.dir.join("interaction-unicode-text.png"));
    st.set_dialog(Dialog::None);
    st.set_tool(Tool::Hand);
    pause(300).await;
    cx.record(
        "text the font lacks: named while typing, refused on OK, nothing changed",
        opened && warned.contains('漢') && warned.contains("U+6F22") && still_open && toast && unchanged,
        format!(
            "opened {opened}, hint {warned:?}, dialog kept {still_open}, toast {toast}, unchanged {unchanged}"
        ),
    );

    // Editing the text of the new text box: the same warning and refusal.
    let rev = revision(st);
    st.set_dialog(Dialog::AnnotationText {
        page: 0,
        index,
        contents: text.to_string(),
    });
    pause(500).await;
    type_text("Łódź שלום").await;
    let warned = hint().await;
    press_ok().await;
    let still_open = matches!((st.dialog)(), Dialog::AnnotationText { .. });
    let kept = annots_on(st, 0)
        .get(index)
        .map(|a| a.contents == text)
        .unwrap_or(false);
    st.set_dialog(Dialog::None);
    pause(300).await;
    cx.record(
        "a text box edited to right-to-left text: named, refused, the text kept",
        warned.contains("U+05E9") && still_open && kept && revision(st) == rev,
        format!("hint {warned:?}, dialog kept {still_open}, text kept {kept}"),
    );

    // An import with a comment that cannot be drawn: named, the others imported on request.
    let xfdf = {
        let mut other = folitecta_core::Document::new_blank(1, 595.0, 842.0).ok();
        if let Some(d) = other.as_mut() {
            let _ = d.add_free_text(
                0,
                [60.0, 60.0, 300.0, 90.0],
                "Ελληνικά",
                "Reviewer",
                12.0,
                [0.0; 3],
                true,
            );
            let _ = d.add_free_text(
                0,
                [60.0, 100.0, 300.0, 130.0],
                "PLACEHOLDER",
                "Reviewer",
                12.0,
                [0.0; 3],
                true,
            );
        }
        other
            .and_then(|d| d.export_comments(folitecta_core::CommentFormat::Xfdf, None).ok())
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .replace("PLACEHOLDER", "漢字")
                    .into_bytes()
            })
            .unwrap_or_default()
    };
    tables_and_tapes(cx).await;

    let count = annots_on(st, 0).len();
    crate::files::import_comment_data(st, xfdf);
    pause(600).await;
    let body = eval("return document.querySelector('.dialog p')?.textContent || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let asked = matches!(
        (st.dialog)(),
        Dialog::Confirm {
            action: crate::state::ConfirmAction::ImportCommentsSkipping(_),
            ..
        }
    );
    let nothing_yet = annots_on(st, 0).len() == count;
    press_ok().await;
    let imported = annots_on(st, 0).len() == count + 1
        && annots_on(st, 0)
            .last()
            .map(|a| a.contents == "Ελληνικά")
            .unwrap_or(false);
    let closed = matches!((st.dialog)(), Dialog::None);
    cx.record(
        "an import names the comments it cannot draw and imports the others on request",
        asked && body.contains('漢') && nothing_yet && imported && closed,
        format!("asked {asked}, body {body:?}, nothing before OK {nothing_yet}, imported {imported}, closed {closed}"),
    );
}

/// Whether comment `index` of page 0 draws with an embedded FreeSans subset only.
fn draws_with_freesans(st: AppState, index: usize) -> bool {
    let fonts = st
        .read_active(|t| t.doc.annotation_fonts(0, index).unwrap_or_default())
        .unwrap_or_default();
    fonts.len() == 1
        && fonts[0].subtype == "Type0"
        && fonts[0].embedded
        && fonts[0].to_unicode
        && fonts[0].base_font.ends_with("+FreeSans")
}

/// Type `value` into the first cell of the open table editor (a bold header cell).
async fn type_first_cell(value: &str) {
    let js = format!(
        "const a = document.querySelector('.table-grid textarea'); if (a) {{ a.value = {}; a.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(value).unwrap_or_default()
    );
    let _ = eval(&js).await;
    pause(400).await;
}

/// Tables and calculator tapes with text beyond WinAnsi (FOL-L018/S02).
async fn tables_and_tapes(cx: &mut Ctx<'_>) {
    let (st, z) = (cx.st, cx.z);
    let text = "Łódź — Ελληνικά";
    // A table drawn with the table tool, its bold header cell typed in
    // Polish and Greek.
    let before = annots_on(st, 0).len();
    crate::commands::dispatch(st, "tool.table.create");
    pause(300).await;
    let _ = pointer_path(0, z, &line((330.0, 600.0), (560.0, 700.0), 6), false).await;
    pause(500).await;
    let asked = matches!((st.dialog)(), Dialog::TableNew { .. });
    press_ok().await;
    let editor = (st.dialog)() == Dialog::TableEditor;
    type_first_cell(text).await;
    let warned = hint().await;
    press_ok().await;
    let closed = matches!((st.dialog)(), Dialog::None);
    let after = annots_on(st, 0);
    let index = after.len().saturating_sub(1);
    let table = st.read_active(|t| t.doc.table_at(0, index)).flatten();
    let typed = table
        .as_ref()
        .is_some_and(|t| t.cells[0][0].text == text && t.cells[0][0].bold);
    let embedded = draws_with_freesans(st, index);
    cx.record(
        "a table typed beyond WinAnsi is drawn with an embedded font subset",
        asked && editor && warned.is_empty() && closed && after.len() == before + 1 && typed && embedded,
        format!(
            "asked {asked}, editor {editor}, hint {warned:?}, closed {closed}, {before} -> {} comments, typed {typed}, embedded {embedded}",
            after.len()
        ),
    );

    // Text the font lacks: named in the editor, refused on OK, the editor
    // kept open with what was typed, nothing changed.
    let rev = revision(st);
    let opened = crate::commands::tables::edit_table(st, 0, index);
    pause(500).await;
    type_first_cell("漢字").await;
    let warned = hint().await;
    press_ok().await;
    let kept_open = (st.dialog)() == Dialog::TableEditor;
    let still_typed = crate::commands::tables::TABLE_EDIT
        .peek()
        .as_ref()
        .is_some_and(|e| e.table.cells[0][0].text == "漢字");
    let toast = error_toast_with(st, "漢");
    let unchanged = revision(st) == rev
        && st
            .read_active(|t| t.doc.table_at(0, index))
            .flatten()
            .is_some_and(|t| t.cells[0][0].text == text);
    let _ = capture_window(&cx.dir.join("interaction-unicode-table.png"));
    *crate::commands::tables::TABLE_EDIT.write() = None;
    st.set_dialog(Dialog::None);
    st.set_tool(Tool::Hand);
    pause(300).await;
    cx.record(
        "a table the font cannot draw: named in the editor, refused on OK, kept open, nothing changed",
        opened && warned.contains('漢') && kept_open && still_typed && toast && unchanged,
        format!(
            "opened {opened}, hint {warned:?}, kept open {kept_open}, typed kept {still_typed}, toast {toast}, unchanged {unchanged}"
        ),
    );

    // A calculator tape whose line has a comment in Polish.
    crate::commands::dispatch(st, "comments.accountingCalc");
    pause(300).await;
    let _ = pointer_path(0, z, &[(380.0, 760.0), (380.0, 760.0)], false).await;
    pause(600).await;
    for (amount, op) in [("12.5", "+"), ("7.5", "+")] {
        let js = format!(
            "const i = document.getElementById('calc-entry'); if (i) {{ i.value = '{amount}'; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}"
        );
        let _ = eval(&js).await;
        pause(150).await;
        let js = format!(
            "[...document.querySelectorAll('.calc-key.op')].find(b => b.textContent.trim() === '{op}')?.click();"
        );
        let _ = eval(&js).await;
        pause(350).await;
    }
    let _ = eval(
        "const i = document.getElementById('calc-comment-0'); if (i) { i.value = 'Zażółć'; i.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(500).await;
    let place = crate::commands::calculator::CALC_EDIT
        .peek()
        .as_ref()
        .map(|e| (e.page, e.index));
    let tape = place.and_then(|(p, i)| st.read_active(|t| t.doc.calculator_at(p, i)).flatten());
    let commented = tape
        .as_ref()
        .is_some_and(|t| t.lines.first().is_some_and(|l| l.comment == "Zażółć"));
    let embedded = place.is_some_and(|(_, i)| draws_with_freesans(st, i));
    crate::commands::dispatch(st, "comments.ac.closeAll");
    pause(300).await;
    st.with_active(|t| t.selected_annot = None);
    cx.record(
        "a calculator comment beyond WinAnsi is drawn with an embedded font subset",
        place.is_some() && commented && embedded,
        format!("tape {place:?}, commented {commented}, embedded {embedded}"),
    );
}
