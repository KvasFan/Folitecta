//! Interaction step 44: the text of a page changed (deleted, typed,
//! replaced), corrections suggested with comments, and shapes shown as they
//! are drawn.

use super::*;
use crate::state::TextSel;

/// Characters of the first occurrence of `word` on a page (text layer).
fn find(st: AppState, page: usize, word: &str) -> Option<(usize, usize)> {
    st.read_active(|t| {
        let text = t.doc.page_text(page);
        let byte = text.find(word)?;
        let start = text[..byte].chars().count();
        Some((start, start + word.chars().count()))
    })
    .flatten()
}

fn count(st: AppState, page: usize, word: &str) -> usize {
    st.read_active(|t| t.doc.page_text(page).matches(word).count())
        .unwrap_or(0)
}

/// Type `value` into the text area of the open dialog and press its main button.
async fn answer(value: &str) {
    let js = format!(
        "const a = document.querySelector('.dialog textarea'); if (a) {{ a.value = {}; a.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(value).unwrap_or_default()
    );
    let _ = document::eval(&js).await;
    pause(250).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir, z) = (cx.st, cx.dir, cx.z);
    // 44. Text of the page: a word deleted with the edit text tool, text
    // typed at the caret, a word replaced from the selection toolbar, a
    // correction suggested as a comment; shapes drawn with their preview.
    st.go_to_page(0);
    crate::commands::dispatch(st, "tool.editContent.text");
    pause(400).await;
    let before = count(st, 0, "quick");
    let deleted = match find(st, 0, "quick ") {
        Some((a, b)) => {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page: 0,
                    anchor: a,
                    head: b,
                })
            });
            crate::commands::textedit::key(st, &Key::Delete);
            pause(800).await;
            count(st, 0, "quick") + 1 == before
        }
        None => false,
    };
    // Typing at a caret opens the dialog with what was typed.
    let typed = match find(st, 0, "Section 1") {
        Some((a, _)) => {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page: 0,
                    anchor: a,
                    head: a,
                })
            });
            crate::commands::textedit::key(st, &Key::Character("N".into()));
            pause(500).await;
            let shown = document::eval("return document.querySelector('.dialog textarea')?.value || '';")
                .await
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            answer("New ").await;
            shown == "N"
                && st
                    .read_active(|t| t.doc.page_text(0).starts_with("New Section 1"))
                    .unwrap_or(false)
        }
        None => false,
    };
    cx.record(
        "the text of the page: a word deleted, text typed at the caret",
        deleted && typed,
        format!(
            "deleted {deleted}, typed {typed}: {:?}",
            st.read_active(|t| t.doc.page_text(0).chars().take(40).collect::<String>())
        ),
    );
    // Replace from the toolbar of the selection.
    let replaced = match find(st, 0, "brown") {
        Some((a, b)) => {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page: 0,
                    anchor: a,
                    head: b,
                })
            });
            pause(400).await;
            let _ = document::eval("document.querySelector('.sel-toolbar .sel-edit')?.click();").await;
            pause(500).await;
            let prefilled = document::eval("return document.querySelector('.dialog textarea')?.value || '';")
                .await
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            let _ = capture_window(&dir.join("interaction-text-replace.png"));
            answer("golden").await;
            prefilled == "brown" && find(st, 0, "quick golden fox").is_some()
                || find(st, 0, "golden fox").is_some()
        }
        None => false,
    };
    cx.record("a word replaced in the text of the page", replaced, String::new());
    // The Edit group of Home: the edit text tool and Delete Text, which takes
    // the selected words out of the page.
    st.set_tool(Tool::Hand);
    st.set_ribbon_tab("home".into());
    pause(400).await;
    let in_home = document::eval("return !!document.querySelector('.ribbon [data-cmd=\"tool.editText\"]');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let overs = count(st, 0, "over ");
    let removed = match find(st, 0, "over ") {
        Some((a, b)) => {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page: 0,
                    anchor: a,
                    head: b,
                })
            });
            crate::commands::dispatch(st, "pageText.delete");
            pause(800).await;
            count(st, 0, "over ") + 1 == overs && count(st, 0, "lazy") > 0
        }
        None => false,
    };
    cx.record(
        "Edit of Home: the edit text tool, a word deleted from the page",
        in_home && removed,
        format!("in home {in_home}, deleted {removed}"),
    );

    // Find and Replace: the next occurrence selected, one replaced, then
    // every other one on the page at once, undone in one step.
    st.with_active(|t| t.text_sel = None);
    crate::commands::dispatch(st, "findAndRepl");
    pause(400).await;
    let set = |sel: &'static str, value: &'static str| async move {
        let js = format!(
            "const i = document.querySelector('.dialog .{sel}'); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
            serde_json::to_string(value).unwrap_or_default()
        );
        let _ = document::eval(&js).await;
        pause(200).await;
    };
    set("findrepl-find", "Pack my").await;
    set("findrepl-with", "Fill my").await;
    let packs = count(st, 0, "Pack my");
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let selected = st
        .read_active(|t| {
            t.text_sel.map(|s| {
                t.doc
                    .page_text(s.page)
                    .chars()
                    .skip(s.range().0)
                    .take(7)
                    .collect::<String>()
            })
        })
        .flatten()
        .unwrap_or_default();
    let _ = document::eval("document.querySelector('.dialog .findrepl-replace')?.click();").await;
    pause(900).await;
    let one = count(st, 0, "Fill my") == 1 && count(st, 0, "Pack my") + 1 == packs;
    let _ = capture_window(&dir.join("interaction-find-replace.png"));
    let _ = document::eval(
        "const r = document.querySelectorAll('.dialog input[name=findrepl-scope]'); if (r[1]) r[1].click();",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog .findrepl-all')?.click();").await;
    pause(2500).await;
    let all = count(st, 0, "Pack my") == 0 && count(st, 0, "Fill my") == packs;
    st.set_dialog(Dialog::None);
    crate::commands::dispatch(st, "undo");
    pause(600).await;
    let undone = count(st, 0, "Fill my") == 1;
    cx.record(
        "Find and Replace: next occurrence, one replaced, all on the page, one undo",
        packs > 1 && selected == "Pack my" && one && all && undone,
        format!("{packs} found, selected {selected:?}, one {one}, all {all}, undone {undone}"),
    );
    // A correction suggested with a comment leaves the text alone.
    crate::commands::dispatch(st, "tool.textCorrectionMarkup");
    let comments = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    let text_before = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
    let suggested = match find(st, 0, "lazy") {
        Some((a, b)) => {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page: 0,
                    anchor: a,
                    head: b,
                })
            });
            st.set_dialog(Dialog::ReplaceText {
                page: 0,
                start: a,
                end: b,
                note: true,
            });
            pause(400).await;
            answer("sleepy").await;
            let after = st
                .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
                .unwrap_or(0);
            after > comments && st.read_active(|t| t.doc.page_text(0)).unwrap_or_default() == text_before
        }
        None => false,
    };
    cx.record("a correction suggested with a comment", suggested, String::new());
    st.set_tool(Tool::Rect);
    pause(300).await;

    // Shapes are drawn as they will look while the pointer moves.
    let preview = |shape: &'static str| async move {
        let js = format!(
            r#"
const el = document.querySelector('#pg-0 .page-inner');
if (!el) return false;
const r = el.getBoundingClientRect();
const at = (x, y) => ({{ bubbles: true, cancelable: true, button: 0, buttons: 1, clientX: r.left + x, clientY: r.top + y }});
el.dispatchEvent(new MouseEvent('mousedown', at({x0}, {y0})));
el.dispatchEvent(new MouseEvent('mousemove', at({x1}, {y1})));
return true;
"#,
            x0 = 80.0 * z,
            y0 = 420.0 * z,
            x1 = 200.0 * z,
            y1 = 500.0 * z
        );
        let _ = document::eval(&js).await;
        pause(300).await;
        let seen = document::eval(&format!(
            "return !!document.querySelector('#pg-0 svg.ink-preview {shape}');"
        ))
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
        let _ = document::eval(
            "const el = document.querySelector('#pg-0 .page-inner'); const r = el.getBoundingClientRect(); el.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientX: r.left + 1, clientY: r.top + 1 }));",
        )
        .await;
        pause(500).await;
        seen
    };
    let rect = preview("rect").await;
    let _ = capture_window(&dir.join("interaction-shape-preview.png"));
    st.set_tool(Tool::Ellipse);
    pause(300).await;
    let ellipse = preview("ellipse").await;
    cx.record(
        "rectangles and ellipses are drawn as they will look",
        rect && ellipse,
        format!("rect {rect}, ellipse {ellipse}"),
    );

    // The pencil: a pencil cursor, the stroke shown as a curve while drawn
    // and kept with more pieces than the pointer made, the tool still chosen
    // for the next stroke.
    st.set_tool(Tool::Ink);
    pause(300).await;
    let cursor = document::eval(
        "const c = document.querySelector('.canvas.tool-pencil'); return c ? getComputedStyle(c).cursor : '';",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default();
    let inks = |st: AppState| {
        annots_on(st, 0)
            .into_iter()
            .filter(|a| a.subtype == "Ink")
            .count()
    };
    let before = inks(st);
    let wave: Vec<(f32, f32)> = (0..=12)
        .map(|i| {
            let t = i as f32 / 12.0;
            (
                100.0 + 220.0 * t,
                610.0 + 25.0 * (t * std::f32::consts::TAU).sin(),
            )
        })
        .collect();
    let pts: Vec<String> = wave
        .iter()
        .map(|(x, y)| format!("[{},{}]", x * z, y * z))
        .collect();
    let js = format!(
        r#"
const el = document.querySelector('#pg-0 .page-inner');
if (!el) return false;
const r = el.getBoundingClientRect();
const pts = [{}];
const ev = (type, p, buttons) => el.dispatchEvent(new MouseEvent(type, {{ bubbles: true, cancelable: true, button: 0, buttons, clientX: r.left + p[0], clientY: r.top + p[1] }}));
ev('mousedown', pts[0], 1);
for (let i = 1; i < pts.length; i++) ev('mousemove', pts[i], 1);
await new Promise(res => setTimeout(res, 300));
const curve = document.querySelector('#pg-0 svg.ink-preview path.ink-stroke');
const shown = !!curve && (curve.getAttribute('d') || '').includes('C');
ev('mouseup', pts[pts.length - 1], 0);
return shown;
"#,
        pts.join(",")
    );
    let curved = document::eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    pause(600).await;
    let pieces = st
        .read_active(|t| {
            let a = t
                .doc
                .annotations(0)
                .ok()?
                .into_iter()
                .rfind(|a| a.subtype == "Ink")?;
            let d = t.doc.inner().get_dictionary(a.id?).ok()?;
            let list = d.get(b"InkList").ok()?.as_array().ok()?;
            Some(list.first()?.as_array().ok()?.len() / 2 - 1)
        })
        .flatten()
        .unwrap_or(0);
    let kept = *st.tool.peek() == Tool::Ink;
    let _ = pointer_path(0, z, &line((120.0, 660.0), (300.0, 680.0), 8), false).await;
    pause(600).await;
    let two = inks(st) == before + 2 && *st.tool.peek() == Tool::Ink;
    cx.record(
        "the pencil: its cursor, a smooth stroke, still chosen after a stroke",
        cursor.contains("url(") && curved && pieces >= 2 * (wave.len() - 1) && kept && two,
        format!(
            "cursor {}, curve {curved}, pieces {pieces}, kept {kept}, two strokes {two}",
            !cursor.is_empty()
        ),
    );
    let _ = capture_window(&dir.join("interaction-pencil.png"));

    // Comment styles: the default rectangle style cloned and named, made red,
    // made the default of the tool, shown in the palette, then reset.
    use crate::commands::comment_styles::STYLES;
    st.set_tool(Tool::Rect);
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "comment-styles".into();
        ui.show_left_pane = true;
    }
    pause(400).await;
    {
        let mut w = STYLES.write();
        w.kind = "Square".into();
        w.chosen = Some(None);
    }
    crate::commands::dispatch(st, "commentStyles.clone");
    pause(400).await;
    let _ = document::eval(
        "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Red frame'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let mut list = crate::state::comment_styles::named_styles(st);
    let named = list.len() == 1 && list[0].name == "Red frame";
    if let Some(n) = list.first_mut() {
        let mut s = n.style();
        s.color = Some([1.0, 0.0, 0.0]);
        *n = crate::state::comment_styles::NamedStyle::new(n.name.clone(), "Square", &s);
    }
    crate::state::comment_styles::save_named_styles(st, &list);
    STYLES.write().chosen = Some(Some(0));
    st.with_active(|t| t.selected_annot = None);
    crate::commands::dispatch(st, "commentStyles.applyStyle");
    pause(300).await;
    let red = crate::state::comment_styles::default_style(st, "Square").color == Some([1.0, 0.0, 0.0]);
    st.set_option("view.commentStyles", true);
    pause(400).await;
    let palette = document::eval("return document.querySelectorAll('.styles-palette .palette-item').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&dir.join("interaction-comment-styles.png"));
    st.set_option("view.commentStyles", false);
    crate::commands::dispatch(st, "commentStyles.reset");
    pause(300).await;
    let reset = crate::state::comment_styles::named_styles(st).is_empty()
        && crate::state::comment_styles::default_style(st, "Square")
            == crate::state::comment_styles::builtin("Square");
    cx.record(
        "comment styles: cloned, named, made the default, in the palette, reset",
        named && red && palette == 2 && reset,
        format!("named {named}, red {red}, palette {palette}, reset {reset}"),
    );
    st.ui_mut().left_pane = "thumbnails".into();

    // Redaction: a pixelated area with a code written over it.
    let mut look = crate::commands::redaction::default_look(st);
    look.overlay = folitecta_core::redaction::Overlay::Pixelate;
    look.text = "CODE-7".into();
    crate::commands::redaction::save_default_look(st, &look);
    let pages = st.page_count();
    let page = pages.saturating_sub(1);
    let applied = st
        .with_active(|t| {
            let look = look.clone();
            t.doc
                .add_redaction_with(page, [100.0, 600.0, 300.0, 640.0], "Reviewer", &look)
                .ok()?;
            t.doc.apply_redactions(Some(&[page])).ok()
        })
        .flatten();
    pause(600).await;
    let coded = st
        .read_active(|t| t.doc.page_text(page).contains("CODE-7"))
        .unwrap_or(false);
    crate::commands::redaction::save_default_look(st, &folitecta_core::redaction::RedactionLook::default());
    cx.record(
        "a redaction pixelated with a code over it",
        applied == Some(1) && coded,
        format!("{applied:?} applied, code {coded}"),
    );
    st.set_tool(Tool::Hand);
    pause(300).await;
}
