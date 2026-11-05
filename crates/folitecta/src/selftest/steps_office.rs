//! Interaction step: Export to Office Formats — the dialog offers the
//! formats; a document exported to Word and to a web page (written beside
//! the self-test's files, as the save dialog would): the headings and the
//! text in them, the picture of the page in both, the document itself
//! unchanged.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let md = "# Annual report\n\nThe first paragraph of the report.\n\n## Results\n\nThe results are good.\n";
    let made = folitecta_core::Document::from_markdown(md, folitecta_core::TextLayout::default(), "Report")
        .and_then(|mut d| {
            let img = image::RgbImage::from_pixel(30, 20, image::Rgb([40, 120, 200]));
            let mut png = std::io::Cursor::new(Vec::new());
            let _ = img.write_to(&mut png, image::ImageFormat::Png);
            d.add_image(0, png.into_inner(), Some([100.0, 300.0, 250.0, 400.0]))?;
            d.clear_history();
            Ok(d)
        });
    let Ok(doc) = made else {
        cx.record("office export", false, "the document could not be made".into());
        return;
    };
    let original = doc.bytes();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "document.exportOffice");
    pause(400).await;
    let formats =
        text("return String(document.querySelectorAll('.dialog .office-format option').length);").await;
    let docx = cx.dir.join("report.docx");
    let html = cx.dir.join("report.html");
    let _ = std::fs::remove_file(&docx);
    let _ = std::fs::remove_file(&html);
    // Word through the dialog itself: chosen, Export pressed, the save
    // window answered.
    let _ = text(
        "const s = document.querySelector('.dialog .office-format'); if (s) { s.value = 'docx'; s.dispatchEvent(new Event('change', { bubbles: true })); } return '';",
    )
    .await;
    pause(300).await;
    crate::commands::office::answer_save(docx.clone());
    let _ = text("document.querySelector('.dialog-foot .tbtn.primary')?.click(); return '';").await;
    crate::commands::office::write_export(st, folitecta_core::OfficeFormat::Html, true, false, html.clone());
    for _ in 0..40 {
        pause(150).await;
        if docx.exists() && html.exists() {
            break;
        }
    }
    let parts = std::fs::read(&docx)
        .ok()
        .and_then(|d| folitecta_core::zip::read(&d).ok())
        .unwrap_or_default();
    let body = parts
        .iter()
        .find(|p| p.name == "word/document.xml")
        .map(|p| String::from_utf8_lossy(&p.data).to_string())
        .unwrap_or_default();
    let media = parts.iter().filter(|p| p.name.starts_with("word/media/")).count();
    let page = std::fs::read_to_string(&html).unwrap_or_default();
    let unchanged = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(200).await;
    cx.record(
        "export to office formats: three formats offered; Word and web page written with the headings, the text and the picture; the document unchanged",
        formats == "3"
            && body.contains("Heading1")
            && body.contains("Annual report")
            && body.contains("The results are good.")
            && page.contains("<h2>Results</h2>")
            && media == 1
            && body.contains("r:embed=\"rIdImg1\"")
            && page.contains("data:image/png;base64,")
            && unchanged,
        format!(
            "formats {formats}, docx {} bytes with {media} pictures, html {} bytes, unchanged {unchanged}",
            body.len(),
            page.len()
        ),
    );
}
