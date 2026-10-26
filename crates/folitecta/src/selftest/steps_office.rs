//! Interaction step: Export to Office Formats — the dialog offers the
//! formats; a document exported to Word and to a web page (written beside
//! the self-test's files, as the save dialog would): the headings and the
//! text in them, the document itself unchanged.

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
    let Ok(doc) =
        folitecta_core::Document::from_markdown(md, folitecta_core::TextLayout::default(), "Report")
    else {
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
    st.set_dialog(crate::state::Dialog::None);
    let docx = cx.dir.join("report.docx");
    let html = cx.dir.join("report.html");
    let _ = std::fs::remove_file(&docx);
    let _ = std::fs::remove_file(&html);
    crate::commands::office::write_export(st, folitecta_core::OfficeFormat::Docx, true, true, docx.clone());
    crate::commands::office::write_export(st, folitecta_core::OfficeFormat::Html, true, false, html.clone());
    for _ in 0..40 {
        pause(150).await;
        if docx.exists() && html.exists() {
            break;
        }
    }
    let body = std::fs::read(&docx)
        .ok()
        .and_then(|d| folitecta_core::zip::read(&d).ok())
        .and_then(|parts| parts.into_iter().find(|p| p.name == "word/document.xml"))
        .map(|p| String::from_utf8_lossy(&p.data).to_string())
        .unwrap_or_default();
    let page = std::fs::read_to_string(&html).unwrap_or_default();
    let unchanged = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(200).await;
    cx.record(
        "export to office formats: three formats offered; Word and web page written with the headings and text; the document unchanged",
        formats == "3"
            && body.contains("Heading1")
            && body.contains("Annual report")
            && body.contains("The results are good.")
            && page.contains("<h2>Results</h2>")
            && unchanged,
        format!("formats {formats}, docx {} bytes, html {} bytes, unchanged {unchanged}", body.len(), page.len()),
    );
}
