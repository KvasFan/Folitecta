//! Interaction step: what is drawn or written goes to the layer selected in
//! the layers pane. A layer chosen with a click on its row receives an ink
//! stroke and a text box (their `/OC` names it in the saved file); hiding the
//! layer in the view hides them in the canvas and in the comments pane; a
//! stroke drawn on the hidden layer shows it again; with no layer chosen a
//! new stroke belongs to none.

use folitecta_core::RenderOpts;

use super::*;

const LAYER: &str = "Ink and notes";

/// The page as the canvas shows it (from the renderer of the tab), with or
/// without the comments.
async fn rendered(st: AppState, page: usize, annotations: bool) -> Option<image::RgbaImage> {
    let (tx, rx) = std::sync::mpsc::channel();
    let opts = RenderOpts {
        annotations,
        transparent: false,
    };
    st.read_active(|t| {
        t.render.render_ext(page, 1.0, opts, move |r| {
            let _ = tx.send(r);
        })
    })?;
    for _ in 0..300 {
        if let Ok(r) = rx.try_recv() {
            return r
                .ok()
                .and_then(|p| image::load_from_memory(&p.png).ok())
                .map(|i| i.to_rgba8());
        }
        pause(50).await;
    }
    None
}

/// Pixels of `area` (display points) where the two pictures differ.
fn changed(a: &image::RgbaImage, b: &image::RgbaImage, area: [f32; 4]) -> usize {
    let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
    let x0 = (area[0].max(0.0) as u32).min(w);
    let x1 = (area[2].ceil().max(0.0) as u32).min(w);
    let y0 = (area[1].max(0.0) as u32).min(h);
    let y1 = (area[3].ceil().max(0.0) as u32).min(h);
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            if (0..3).any(|c| p[c].abs_diff(q[c]) > 40) {
                n += 1;
            }
        }
    }
    n
}

/// The layer named `name` of the `/OC` of the annotations of page 0 of a
/// saved file, by subtype.
fn saved_layers(path: &std::path::Path) -> Result<Vec<(String, Option<String>)>, String> {
    let doc = folitecta_core::Document::open(path).map_err(|e| e.to_string())?;
    let names: std::collections::HashMap<_, _> = doc.layers().into_iter().map(|l| (l.id, l.name)).collect();
    let annots = doc.annotations(0).map_err(|e| e.to_string())?;
    Ok(annots
        .iter()
        .map(|a| {
            let layer =
                a.id.and_then(|id| doc.inner().get_dictionary(id).ok())
                    .and_then(|d| d.get(b"OC").ok())
                    .and_then(|o| o.as_reference().ok())
                    .and_then(|id| names.get(&id).cloned());
            (a.subtype.clone(), layer)
        })
        .collect())
}

fn last_annot(st: AppState) -> Option<folitecta_core::Annotation> {
    annots_on(st, 0).pop()
}

async fn draw_stroke(cx: &Ctx<'_>, y: f32) -> String {
    cx.st.set_tool(Tool::Ink);
    pause(300).await;
    let wave: Vec<(f32, f32)> = (0..16)
        .map(|i| (330.0 + i as f32 * 9.0, y + ((i as f32) * 0.9).sin() * 14.0))
        .collect();
    let r = pointer_path(0, cx.z, &wave, false).await;
    pause(700).await;
    r
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    st.go_to_page(0);
    st.set_tool(Tool::Hand);
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "layers".into();
        ui.show_left_pane = true;
    }
    pause(400).await;

    // A layer, chosen with a click on its row in the layers pane.
    let made = st.with_active(|t| t.doc.add_layer(LAYER, &[]).map(|(id, _)| id));
    let Some(Ok(layer)) = made else {
        cx.record("layer for drawing created", false, format!("{made:?}"));
        return;
    };
    pause(500).await;
    let clicked = eval(&format!(
        "const r = [...document.querySelectorAll('.layer-row')].find(e => e.title === {}); if (r) r.click(); return !!r;",
        serde_json::to_string(LAYER).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    pause(500).await;
    let drawing = st.read_active(|t| t.doc.drawing_layer()).flatten();
    let marks = eval(
        "return [!!document.querySelector('.layer-row.drawing .drawing-mark'), document.querySelector('.statusbar .layer-chip')?.textContent || ''];",
    )
    .await
    .unwrap_or_default();
    let chip = marks.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
    cx.record(
        "a layer selected in the pane becomes the drawing layer",
        clicked
            && drawing == Some(layer)
            && marks.get(0).and_then(|v| v.as_bool()) == Some(true)
            && chip.contains(LAYER),
        format!("clicked {clicked}, drawing {drawing:?}, chip {chip:?}"),
    );

    // An ink stroke and a text box drawn on it.
    let r = draw_stroke(cx, 470.0).await;
    let ink = last_annot(st).filter(|a| a.subtype == "Ink");
    let ink_layer = ink
        .as_ref()
        .and_then(|a| st.read_active(|t| t.doc.annotation_layer(0, a.index)).flatten());
    cx.record(
        "an ink stroke goes to the drawing layer",
        ink_layer == Some(layer),
        format!("{r}; {:?} on {ink_layer:?}", ink.as_ref().map(|a| a.index)),
    );
    crate::commands::dispatch(st, "tool.annot.textBox");
    pause(300).await;
    let _ = pointer_path(0, cx.z, &line((330.0, 530.0), (480.0, 580.0), 5), false).await;
    pause(600).await;
    let dialog_open = matches!((st.dialog)(), Dialog::TextBox { border: true, .. });
    let _ = eval(
        "const a = document.querySelector('.dialog textarea'); if (a) { a.value = 'On the layer'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(800).await;
    let text = last_annot(st).filter(|a| a.subtype == "FreeText");
    let text_layer = text
        .as_ref()
        .and_then(|a| st.read_active(|t| t.doc.annotation_layer(0, a.index)).flatten());
    cx.record(
        "a text box goes to the drawing layer",
        dialog_open && text_layer == Some(layer),
        format!(
            "dialog {dialog_open}; {:?} on {text_layer:?}",
            text.as_ref().map(|a| a.index)
        ),
    );

    // The saved file names the layer in the /OC of both.
    let path = dir.join("layer-drawing.pdf");
    let saved = st
        .read_active(|t| {
            let mut copy = folitecta_core::Document::from_bytes(t.doc.bytes().to_vec(), None, None)
                .map_err(|e| e.to_string())?;
            copy.save_as(&path).map_err(|e| e.to_string())
        })
        .unwrap_or_else(|| Err("no document".into()))
        .and_then(|_| saved_layers(&path));
    let on_layer = |kind: &str| {
        saved
            .as_ref()
            .ok()
            .and_then(|l| l.iter().rev().find(|(s, _)| s == kind).cloned())
            .and_then(|(_, n)| n)
    };
    cx.record(
        "the saved file puts the stroke and the text box on the layer",
        on_layer("Ink").as_deref() == Some(LAYER) && on_layer("FreeText").as_deref() == Some(LAYER),
        format!("{:?}", saved.as_ref().map(|l| l.len())),
    );

    // Hidden in the view: gone from the canvas and from the comments pane.
    st.set_tool(Tool::Hand);
    let size = st.read_active(|t| t.page_sizes[0]).unwrap();
    let area = ink
        .as_ref()
        .map(|a| folitecta_core::marks::pdf_rect_to_display(&size, a.rect))
        .unwrap_or([0.0; 4]);
    let plain = rendered(st, 0, false).await;
    let shown = rendered(st, 0, true).await;
    let src_before = eval("return document.querySelector('#pg-0 img')?.getAttribute('src') || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let _ = eval("document.querySelector('.layer-row.drawing .ibtn.eye')?.click();").await;
    pause(900).await;
    let hidden = rendered(st, 0, true).await;
    let src_after = eval("return document.querySelector('#pg-0 img')?.getAttribute('src') || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    // The stroke against the page without comments, and what hiding took away
    // (other comments of earlier steps may lie in the same area).
    let (drawn, gone, left) = match (&plain, &shown, &hidden) {
        (Some(p), Some(s), Some(h)) => (changed(p, s, area), changed(s, h, area), changed(p, h, area)),
        _ => (0, 0, usize::MAX),
    };
    let in_pane = ink
        .as_ref()
        .and_then(|a| a.id)
        .map(|id| {
            st.read_active(|t| !t.layer_hidden_annots().contains(&id))
                .unwrap_or(true)
        })
        .unwrap_or(true);
    cx.record(
        "hiding the layer hides the stroke in the canvas and the comments pane",
        drawn > 20 && gone * 10 >= drawn * 9 && src_before != src_after && !in_pane,
        format!(
            "{drawn} pixels of the stroke shown, {gone} gone when hidden ({left} differ from the page without comments); image {src_before} -> {src_after}; listed {in_pane}"
        ),
    );
    let _ = capture_window(&dir.join("interaction-layer-drawing.png"));

    // Drawing on the hidden layer shows it again, with a message.
    let _ = draw_stroke(cx, 640.0).await;
    let (visible, noted) = st
        .read_active(|t| t.layer_view.get(&layer).copied().unwrap_or(true))
        .map(|v| (v, st.toasts.read().iter().any(|m| m.title.contains(LAYER))))
        .unwrap_or((false, false));
    cx.record(
        "drawing on a hidden layer shows it again in the view",
        visible && noted,
        format!("visible {visible}, message {noted}"),
    );

    // No layer: the chip clears it, and the next stroke belongs to none.
    let _ = eval("document.querySelector('.statusbar .layer-chip .chip-x')?.click();").await;
    pause(500).await;
    let cleared = st
        .read_active(|t| t.doc.drawing_layer().is_none())
        .unwrap_or(false);
    let _ = draw_stroke(cx, 720.0).await;
    let last = last_annot(st).filter(|a| a.subtype == "Ink");
    let none = last
        .as_ref()
        .map(|a| {
            st.read_active(|t| t.doc.annotation_layers(0, a.index).is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let chip_gone = eval("return !document.querySelector('.statusbar .layer-chip');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    cx.record(
        "with no drawing layer a stroke belongs to no layer",
        cleared && none && chip_gone,
        format!("cleared {cleared}, no layer {none}, chip gone {chip_gone}"),
    );

    st.set_tool(Tool::Hand);
    st.with_active(|t| {
        t.selected_layers.clear();
        t.set_layer_view(std::collections::HashMap::new());
    });
    st.ui_mut().left_pane = "thumbnails".into();
    pause(300).await;
}
