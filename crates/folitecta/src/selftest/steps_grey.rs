//! Interaction step: grey — View › Grey Preview shows the pages in grey and
//! leaves the document as it was; Convert Colours to Grey shows what would
//! change, then converts (the pictures grey, no RGB colour left in the
//! page content); Undo brings the colours back.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn rgb_left(t: &crate::state::DocTab) -> bool {
    let inner = t.doc.inner();
    let pid = t.doc.page_ids()[0];
    let content = String::from_utf8_lossy(&inner.get_page_content(pid)).to_string();
    let images_in_colour = inner
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .any(|s| {
            s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Image".as_slice())
                && s.dict.get(b"ColorSpace").ok().and_then(|c| c.as_name().ok())
                    == Some(b"DeviceRGB".as_slice())
        });
    content.contains(" rg") || content.contains(" RG") || images_in_colour
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::from_template(
        folitecta_core::Template::Invoice,
        &folitecta_core::TemplateOptions::default(),
    )
    .and_then(|mut d| {
        let img = image::RgbImage::from_fn(60, 40, |i, j| image::Rgb([(i * 4) as u8, 50, (j * 6) as u8]));
        let mut png = std::io::Cursor::new(Vec::new());
        let _ = img.write_to(&mut png, image::ImageFormat::Png);
        d.add_image(0, png.into_inner(), Some([300.0, 600.0, 380.0, 660.0]))?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("grey", false, "the document could not be made".into());
        return;
    };
    let original = doc.bytes();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    // The preview: the page pictures in grey, the document unchanged.
    crate::commands::dispatch(st, "view.greyPreview");
    pause(500).await;
    let filter = text("const i = document.querySelector('#canvas img.page-img, #canvas img.page-preview, #canvas img.page-tile'); return i ? (i.getAttribute('style') || '') : '';").await;
    let unchanged = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);
    crate::commands::dispatch(st, "view.greyPreview");
    pause(300).await;
    // The conversion.
    let coloured_before = st.read_active(rgb_left).unwrap_or(false);
    crate::commands::dispatch(st, "document.convertToGrey");
    let mut summary = String::new();
    for _ in 0..40 {
        pause(150).await;
        summary = text("return document.querySelector('.dialog .grey-summary')?.textContent || '';").await;
        if summary.chars().any(|c| c.is_ascii_digit()) {
            break;
        }
    }
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let coloured_after = st.read_active(rgb_left).unwrap_or(true);
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let back = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "grey: a preview that leaves the document as it was; colours converted after an estimate, undone",
        filter.contains("grayscale") && unchanged && coloured_before && !summary.is_empty() && !coloured_after && back,
        format!("filter {filter:?}, unchanged {unchanged}, before {coloured_before}, summary {summary:?}, after {coloured_after}, back {back}"),
    );
}
