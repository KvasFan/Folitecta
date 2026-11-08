//! Interaction step: picture_field — the Picture Field tool is chosen from
//! its command; a picture field clicked while the form is filled asks for a
//! picture (answered as the file window would) and shows it, one undo step.

use folitecta_core::{FieldProps, NewField};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    crate::commands::dispatch(st, "tool.editFields.mode.image");
    pause(200).await;
    let tool_ok = *st.tool.peek() == crate::state::Tool::FieldPicture;
    crate::commands::dispatch(st, "tool.hand");
    pause(200).await;

    let made = folitecta_core::Document::new_blank(1, 400.0, 300.0).and_then(|mut d| {
        let id = d.add_form_field(
            0,
            NewField::Picture,
            [60.0, 60.0, 260.0, 220.0],
            &FieldProps::default(),
        )?;
        d.clear_history();
        Ok((d, id))
    });
    let Ok((doc, field)) = made else {
        cx.record("picture field", false, "the document could not be made".into());
        return;
    };
    let picture = cx.dir.join("picture-field.png");
    let img = image::RgbImage::from_fn(40, 30, |x, _| image::Rgb([200, (x * 6) as u8, 40]));
    let _ = img.save(&picture);
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    let widget = st
        .read_active(|t| t.doc.page_widgets(0).into_iter().next())
        .flatten();
    let picture_field = st.read_active(|t| t.doc.is_picture_field(field)).unwrap_or(false);
    // The Fields pane calls it a picture field.
    crate::commands::dispatch(st, "view.fields");
    pause(400).await;
    let pane_kind = eval("return Array.from(document.querySelectorAll('.field-row .chip, .tree-item .chip')).map(c => c.textContent).join('|');")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let named_picture = pane_kind.contains(&crate::i18n::t(st.lang(), "fieldkind.picture"));
    crate::commands::dispatch(st, "view.fields");
    pause(200).await;
    crate::commands::buttons::answer_picture(picture.clone());
    if let Some(w) = &widget {
        crate::ui::features::form_fill::fill_field(st, 0, w);
    }
    let mut shown = false;
    for _ in 0..30 {
        pause(150).await;
        shown = st.read_active(|t| t.doc.button_icons(field)[0]).unwrap_or(false);
        if shown {
            break;
        }
    }
    pause(300).await;
    let _ = capture_window(&cx.dir.join("interaction-picture-field.png"));
    let undone = st
        .with_active(|t| t.doc.undo().is_ok() && !t.doc.button_icons(field)[0])
        .unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "picture field: the tool is chosen; a picture field clicked while filling takes the picture chosen and shows it, one undo step",
        tool_ok && picture_field && named_picture && widget.is_some() && shown && undone,
        format!(
            "tool {tool_ok}, picture field {picture_field}, in the pane {pane_kind:?}, widget {}, shown {shown}, undone {undone}",
            widget.is_some()
        ),
    );
}
