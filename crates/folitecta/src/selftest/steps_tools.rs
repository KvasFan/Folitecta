//! Interaction steps 1–19: text selection, drawing and editing comments, stamps, ink, notes, measurements, callouts, the eraser, text edits, styles and form fields.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir, z) = (cx.st, cx.dir, cx.z);
    // 1. Text selection by dragging with the text tool.
    st.set_tool(Tool::Select);
    pause(300).await;
    let r = pointer_path(0, z, &line((58.0, 62.0), (300.0, 64.0), 8), false).await;
    pause(500).await;
    let sel = st
        .read_active(|t| {
            t.text_sel.map(|s| {
                (
                    s.page,
                    s.range(),
                    t.doc.text_range(s.page, s.range().0, s.range().1),
                )
            })
        })
        .flatten();
    cx.record(
        "select text by dragging",
        sel.as_ref()
            .map(|(p, (a, b), _)| *p == 0 && b > a)
            .unwrap_or(false),
        format!("{r}; {sel:?}"),
    );

    // 2. Highlight the selection from the floating toolbar command.
    let before = annots_on(st, 0).len();
    crate::commands::dispatch(st, "textsel.addHighlight");
    pause(500).await;
    let after = annots_on(st, 0);
    cx.record(
        "highlight selected text",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Highlight").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 3. Draw a rectangle.
    st.set_tool(Tool::Rect);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r = pointer_path(0, z, &line((100.0, 300.0), (260.0, 380.0), 6), false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    let rect_ok = after.len() == before + 1 && after.last().map(|a| a.subtype == "Square").unwrap_or(false);
    cx.record(
        "draw a rectangle",
        rect_ok,
        format!("{r}; {before} -> {}; tool now {:?}", after.len(), (st.tool)()),
    );

    // 4. Move it with the comment editing tool.
    st.set_tool(Tool::EditAnnots);
    pause(500).await;
    let size = st.read_active(|t| t.page_sizes[0]).unwrap();
    if let Some(a) = annots_on(st, 0).last().cloned() {
        let d = folitecta_core::marks::pdf_rect_to_display(&size, a.rect);
        let c = ((d[0] + d[2]) / 2.0, (d[1] + d[3]) / 2.0);
        let r = pointer_path(0, z, &line(c, (c.0 + 40.0, c.1 + 30.0), 6), true).await;
        pause(600).await;
        let moved = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| folitecta_core::marks::pdf_rect_to_display(&size, x.rect));
        let ok = moved
            .map(|m| ((m[0] - d[0]) - 40.0).abs() < 1.5 && ((m[1] - d[1]) - 30.0).abs() < 1.5)
            .unwrap_or(false);
        cx.record(
            "move a comment by dragging",
            ok,
            format!("{r}; before {d:?} after {moved:?}"),
        );

        // 5. Resize it from the bottom-right handle.
        let selected = st.read_active(|t| t.selected_annot).flatten();
        if let Some(m) = moved {
            let corner = (m[2], m[3]);
            let r = pointer_path(0, z, &line(corner, (corner.0 + 30.0, corner.1 + 20.0), 5), true).await;
            pause(600).await;
            let resized = annots_on(st, 0)
                .into_iter()
                .find(|x| x.index == a.index)
                .map(|x| folitecta_core::marks::pdf_rect_to_display(&size, x.rect));
            let ok = resized
                .map(|n| {
                    ((n[2] - n[0]) - (m[2] - m[0]) - 30.0).abs() < 1.5
                        && ((n[3] - n[1]) - (m[3] - m[1]) - 20.0).abs() < 1.5
                })
                .unwrap_or(false);
            cx.record(
                "resize a comment with a handle",
                ok,
                format!("{r}; selected {selected:?}; before {m:?} after {resized:?}"),
            );
        }

        // 6. Nudge with the keyboard, then delete with the Delete key.
        st.with_active(|t| t.selected_annot = Some((0, a.index)));
        let before_rect = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| x.rect);
        let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true, bubbles: true }));").await;
        pause(500).await;
        let after_rect = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| x.rect);
        let ok = match (before_rect, after_rect) {
            (Some(b), Some(n)) => ((n[0] - b[0]) - 10.0).abs() < 0.6,
            _ => false,
        };
        cx.record(
            "nudge the selected comment with Shift+Right",
            ok,
            format!("{before_rect:?} -> {after_rect:?}"),
        );
        let before = annots_on(st, 0).len();
        st.with_active(|t| t.selected_annot = Some((0, a.index)));
        let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }));").await;
        pause(500).await;
        cx.record(
            "delete the selected comment with the Delete key",
            annots_on(st, 0).len() + 1 == before,
            format!("{before} -> {}", annots_on(st, 0).len()),
        );
    } else {
        cx.record(
            "move a comment by dragging",
            false,
            "no annotation to move".into(),
        );
    }

    // 7. Stamp with a single click.
    st.set_value("stamp.kind", "text");
    st.set_value("stamp.text", "REVIEWED");
    st.set_tool(Tool::Stamp);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r = pointer_path(0, z, &[(300.0, 500.0), (300.0, 500.0)], false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    cx.record(
        "place a stamp with a click",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Stamp").unwrap_or(false),
        format!("{r}; {before} -> {}", after.len()),
    );

    // 8. Freehand ink.
    st.set_tool(Tool::Ink);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let wave: Vec<(f32, f32)> = (0..20)
        .map(|i| (120.0 + i as f32 * 8.0, 600.0 + ((i as f32) * 0.8).sin() * 20.0))
        .collect();
    let r = pointer_path(0, z, &wave, false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    cx.record(
        "draw with the pencil",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Ink").unwrap_or(false),
        format!("{r}; {before} -> {}", after.len()),
    );

    // 9. Sticky note opens its dialog.
    st.set_tool(Tool::Note);
    pause(300).await;
    let _ = pointer_path(0, z, &[(400.0, 200.0), (400.0, 200.0)], false).await;
    pause(500).await;
    let dialog = (st.dialog)();
    cx.record(
        "sticky note tool opens the note dialog",
        matches!(dialog, Dialog::Note { page: 0, .. }),
        format!("{dialog:?}"),
    );
    st.set_dialog(Dialog::None);

    // 10. Text highlight tool marks text directly.
    st.set_tool(Tool::TextHighlight);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let _ = pointer_path(0, z, &line((58.0, 120.0), (250.0, 122.0), 6), false).await;
    pause(700).await;
    let after = annots_on(st, 0);
    cx.record(
        "highlight text tool",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Highlight").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 11. Snapshot copies an area to the clipboard.
    st.set_tool(Tool::Snapshot);
    pause(300).await;
    let _ = pointer_path(0, z, &line((50.0, 50.0), (250.0, 150.0), 4), false).await;
    pause(900).await;
    let png = crate::clipboard::get_png();
    cx.record(
        "snapshot tool copies an image",
        png.as_ref().map(|p| p.len() > 100).unwrap_or(false),
        format!("{} bytes", png.map(|p| p.len()).unwrap_or(0)),
    );

    // 12. Polygon with three clicks and a double click.
    crate::commands::dispatch(st, "tool.annot.polygon");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(100.0, 650.0), (200.0, 640.0), (160.0, 720.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(150).await;
    }
    let _ = document::eval("const el = document.querySelector('#pg-0 .page-inner'); const r = el.getBoundingClientRect(); el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: r.left + 160, clientY: r.top + 720 }));").await;
    pause(600).await;
    let after = annots_on(st, 0);
    cx.record(
        "polygon with clicks and a double click",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Polygon").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 13. Distance measurement by dragging (72 pt = 25.4 mm).
    crate::commands::dispatch(st, "tool.annot.distance");
    pause(300).await;
    let _ = pointer_path(0, z, &line((300.0, 650.0), (372.0, 650.0), 5), false).await;
    pause(600).await;
    let last = annots_on(st, 0).last().cloned();
    cx.record(
        "distance measurement",
        last.as_ref()
            .map(|a| a.subtype == "Line" && a.contents.ends_with("mm"))
            .unwrap_or(false),
        format!("{:?}", last.map(|a| a.contents)),
    );

    // 14. Area with Enter to finish, cloud with Escape to cancel.
    crate::commands::dispatch(st, "tool.annot.area");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(420.0, 640.0), (500.0, 640.0), (500.0, 700.0), (420.0, 700.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(120).await;
    }
    let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));").await;
    pause(600).await;
    let after = annots_on(st, 0);
    cx.record(
        "area measurement finished with Enter",
        after.len() == before + 1
            && after
                .last()
                .map(|a| a.contents.contains('\u{b2}'))
                .unwrap_or(false),
        format!(
            "{before} -> {}; {:?}",
            after.len(),
            after.last().map(|a| a.contents.clone())
        ),
    );
    crate::commands::dispatch(st, "tool.annot.cloud");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(100.0, 750.0), (200.0, 750.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(120).await;
    }
    let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));").await;
    pause(400).await;
    cx.record(
        "Escape cancels a shape being drawn",
        annots_on(st, 0).len() == before && st.drawing.peek().is_none(),
        String::new(),
    );

    // 15. Callout: drag from the pointed spot to where the box goes, then type.
    crate::commands::dispatch(st, "tool.annot.callout");
    pause(300).await;
    let _ = pointer_path(0, z, &line((80.0, 400.0), (250.0, 330.0), 5), false).await;
    pause(500).await;
    let opened = matches!((st.dialog)(), Dialog::Callout { page: 0, .. });
    st.set_dialog(Dialog::None);
    cx.record("callout tool opens its text dialog", opened, String::new());

    // 16. Eraser cuts a pencil stroke.
    crate::commands::dispatch(st, "tool.annot.pencil");
    pause(300).await;
    let stroke: Vec<(f32, f32)> = (0..30).map(|i| (100.0 + i as f32 * 8.0, 560.0)).collect();
    let _ = pointer_path(0, z, &stroke, false).await;
    pause(500).await;
    let inks_before = annots_on(st, 0).iter().filter(|a| a.subtype == "Ink").count();
    crate::commands::dispatch(st, "tool.annot.eraser");
    pause(300).await;
    let _ = pointer_path(0, z, &line((220.0, 540.0), (220.0, 580.0), 4), false).await;
    pause(600).await;
    let ink_paths = st
        .read_active(|t| {
            let a = t
                .doc
                .annotations(0)
                .ok()?
                .into_iter()
                .rfind(|a| a.subtype == "Ink")?;
            let d = t.doc.inner().get_dictionary(a.id?).ok()?;
            Some(d.get(b"InkList").ok()?.as_array().ok()?.len())
        })
        .flatten();
    cx.record(
        "eraser splits a pencil stroke",
        inks_before >= 1 && ink_paths == Some(2),
        format!("ink comments {inks_before}, strokes after erasing {ink_paths:?}"),
    );

    // 17. Insert text and replace text open their dialogs.
    crate::commands::dispatch(st, "tool.annot.insertText");
    pause(300).await;
    let _ = pointer_path(0, z, &[(70.0, 92.0), (70.0, 92.0)], false).await;
    pause(500).await;
    let insert = matches!((st.dialog)(), Dialog::InsertText { page: 0, .. });
    st.set_dialog(Dialog::None);
    cx.record(
        "insert text tool opens its dialog",
        insert,
        format!("{:?}", (st.dialog)()),
    );
    crate::commands::dispatch(st, "tool.annot.replaceText");
    pause(300).await;
    let _ = pointer_path(0, z, &line((58.0, 92.0), (200.0, 92.0), 6), false).await;
    pause(500).await;
    let replace = matches!((st.dialog)(), Dialog::ReplaceText { page: 0, .. });
    st.set_dialog(Dialog::None);
    cx.record("replace text tool opens its dialog", replace, String::new());

    // 18. Style editor: select the polygon and change its border colour.
    st.set_tool(Tool::EditAnnots);
    let poly = annots_on(st, 0)
        .into_iter()
        .find(|a| a.subtype == "Polygon")
        .map(|a| a.index);
    if let Some(i) = poly {
        st.with_active(|t| t.selected_annot = Some((0, i)));
        st.ui_mut().show_right_pane = true;
        pause(700).await;
        let _ = document::eval("const c = document.querySelector('.style-editor input[type=color]'); if (c) { c.value = '#00aa00'; c.dispatchEvent(new Event('change', { bubbles: true })); }").await;
        pause(700).await;
        let color = st
            .read_active(|t| t.doc.annotation_style(0, i).ok().and_then(|s| s.color))
            .flatten();
        cx.record(
            "style editor changes the border colour",
            color
                .map(|c| c[0] < 0.05 && (c[1] - 0.667).abs() < 0.02)
                .unwrap_or(false),
            format!("{color:?}"),
        );
        let _ = capture_window(&dir.join("interaction-style-editor.png"));
        st.ui_mut().show_right_pane = false;
    } else {
        cx.record(
            "style editor changes the border colour",
            false,
            "no polygon".into(),
        );
    }

    // 19. Form fields: create a text field and a check box, fill them on the page.
    crate::commands::dispatch(st, "tool.editFields.mode.text");
    pause(300).await;
    let _ = pointer_path(0, z, &line((300.0, 150.0), (500.0, 172.0), 4), false).await;
    pause(600).await;
    let fields = st.read_active(|t| t.doc.form_fields()).unwrap_or_default();
    cx.record(
        "text field created by dragging",
        fields.len() == 1 && (st.tool)() == Tool::EditFields,
        format!("{} field(s), tool {:?}", fields.len(), (st.tool)()),
    );
    crate::commands::dispatch(st, "tool.editFields.mode.check");
    pause(300).await;
    let _ = pointer_path(0, z, &[(300.0, 190.0), (300.0, 190.0)], false).await;
    pause(600).await;
    let fields = st.read_active(|t| t.doc.form_fields()).unwrap_or_default();
    cx.record(
        "check box created with a click",
        fields.len() == 2,
        format!("{:?}", fields.iter().map(|f| f.kind.label()).collect::<Vec<_>>()),
    );
    // Properties pane: rename the selected check box.
    st.ui_mut().show_right_pane = true;
    pause(700).await;
    let _ = document::eval("const n = document.querySelector('.field-editor input.wide'); if (n) { n.value = 'Accept'; n.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(600).await;
    let renamed = st
        .read_active(|t| t.doc.form_fields().iter().any(|f| f.name == "Accept"))
        .unwrap_or(false);
    cx.record(
        "field renamed in the properties pane",
        renamed,
        format!(
            "{:?}",
            st.read_active(|t| t
                .doc
                .form_fields()
                .into_iter()
                .map(|f| f.name)
                .collect::<Vec<_>>())
        ),
    );
    let _ = capture_window(&dir.join("interaction-field-editor.png"));
    st.ui_mut().show_right_pane = false;
    // Fill the text field on the page and tick the box.
    crate::commands::dispatch(st, "tool.hand");
    pause(500).await;
    let _ = document::eval(
        "const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[0]) h[0].click();",
    )
    .await;
    pause(500).await;
    let editing = st.editing_field.peek().is_some();
    let _ = document::eval("const i = document.querySelector('#pg-0 .field-input'); if (i) { i.value = 'Ada Lovelace'; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); }").await;
    pause(600).await;
    let _ = document::eval(
        "const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[1]) h[1].click();",
    )
    .await;
    pause(600).await;
    let values: Vec<String> = st
        .read_active(|t| t.doc.form_fields().into_iter().map(|f| f.value).collect())
        .unwrap_or_default();
    cx.record(
        "fill a text field and tick a check box on the page",
        editing && values == ["Ada Lovelace", "Yes"],
        format!("editor opened {editing}; values {values:?}"),
    );
    let _ = capture_window(&dir.join("interaction-filled-form.png"));
}
