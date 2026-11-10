//! Interaction step: page_select — Select › Shape chooses the pages that draw
//! shapes; Advanced Selection chooses the pages holding pictures or comments
//! (one of them enough), then adds the odd pages among the first three. Zoom
//! Slider and Zoom Tools reach the zoom slider and the zoom tool.

use super::*;

/// Tick or set a control of the open dialog.
async fn set(selector: &str, js: &str) {
    let _ = eval(&format!(
        "const c = document.querySelector('.dialog {selector}'); if (c) {{ {js} }}"
    ))
    .await;
    pause(150).await;
}

async fn ok() {
    let _ = eval("const b = [...document.querySelectorAll('.dialog button')].find(b => b.classList.contains('primary')); if (b) b.click();").await;
    pause(500).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // Five pages of text: a picture on page 2, a comment on page 4, a
    // circle drawn on page 5.
    let made = folitecta_core::Document::from_bytes(folitecta_core::testdocs::text_doc(5), None, None)
        .and_then(|mut doc| {
            let mut png = std::io::Cursor::new(Vec::new());
            let _ = image::RgbaImage::from_pixel(20, 20, image::Rgba([200, 30, 30, 255]))
                .write_to(&mut png, image::ImageFormat::Png);
            doc.add_image(1, png.into_inner(), Some([100.0, 100.0, 160.0, 160.0]))?;
            doc.add_shape(
                3,
                folitecta_core::Shape::Square,
                [100.0, 100.0, 200.0, 200.0],
                [1.0, 0.0, 0.0],
                None,
                2.0,
                "t",
                "",
            )?;
            doc.add_three_point_shape(
                4,
                folitecta_core::ThreePoint::Circle,
                [[100.0, 100.0], [150.0, 150.0], [200.0, 100.0]],
                [0.0; 3],
                1.0,
            )?;
            Ok(doc)
        });
    let Ok(doc) = made else {
        cx.record(
            "advanced selection",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    let selected = |st: AppState| st.read_active(|t| t.selected_pages.clone()).unwrap_or_default();

    crate::commands::dispatch(st, "thumbnailsView.select.shape");
    pause(300).await;
    let shapes = selected(st);

    crate::commands::dispatch(st, "thumbnailsView.select.with");
    pause(500).await;
    let open = matches!((st.dialog)(), crate::state::Dialog::PageSelection);
    set(".page-select-pictures", "c.click();").await;
    set(".page-select-comments", "c.click();").await;
    set(".page-select-any", "c.click();").await;
    ok().await;
    let holding = selected(st);

    crate::commands::dispatch(st, "thumbnailsView.select.with");
    pause(500).await;
    set(
        ".page-select-parity",
        "c.value = 'odd'; c.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    set(
        ".page-select-range",
        "c.value = '1-3'; c.dispatchEvent(new Event('input', { bubbles: true }));",
    )
    .await;
    set(".page-select-add", "c.click();").await;
    ok().await;
    let added = selected(st);

    // Zoom Slider focuses the slider of the status bar; Zoom Tools chooses
    // the zoom tool.
    crate::commands::dispatch(st, "zoomSlider");
    pause(300).await;
    let slider = eval("return document.activeElement?.id === 'folitecta-zoom-slider';")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    crate::commands::dispatch(st, "tools.zoom");
    pause(200).await;
    let zoom_tool = (st.tool)() == Tool::Zoom;
    st.set_tool(Tool::Hand);
    cx.record(
        "Zoom Slider focuses the zoom slider of the status bar; Zoom Tools chooses the zoom tool",
        slider && zoom_tool,
        format!("slider focused {slider}, zoom tool {zoom_tool}"),
    );
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "pages selected: Shape the page drawing a circle; Advanced Selection the pages with pictures or comments, then the odd ones of 1-3 added",
        open && shapes == vec![4] && holding == vec![1, 3] && added == vec![0, 1, 2, 3],
        format!("open {open}, shapes {shapes:?}, holding {holding:?}, added {added:?}"),
    );
}
