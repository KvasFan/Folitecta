//! Interaction step: Export Tables — the dialog lists the table of a page
//! set from CSV with its size; exported as a workbook through the dialog
//! (numbers as numbers, the sheet named by its page) and as CSV (the cells
//! as they read), the document itself unchanged.

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
    let csv = "Fruit,Price,Share\nApples,\"1,234.50\",12%\nPears,7.25,8.5%\nPlums,(3.10),80%\n";
    let made = folitecta_core::Document::from_csv(csv, None, folitecta_core::TextLayout::default(), "Fruit")
        .map(|mut d| {
            d.clear_history();
            d
        });
    let Ok(doc) = made else {
        cx.record("table export", false, "the document could not be made".into());
        return;
    };
    let original = doc.bytes();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "document.exportTables");
    let mut items = String::new();
    for _ in 0..40 {
        pause(150).await;
        items = text(
            "return Array.from(document.querySelectorAll('.dialog .sheets-table')).map(e => e.textContent.trim()).join('|');",
        )
        .await;
        if !items.is_empty() {
            break;
        }
    }
    let _ = capture_window(&cx.dir.join("interaction-table-export.png"));
    let xlsx = cx.dir.join("fruit.xlsx");
    let csv_out = cx.dir.join("fruit.csv");
    let _ = std::fs::remove_file(&xlsx);
    let _ = std::fs::remove_file(&csv_out);
    let _ = text(
        "const s = document.querySelector('.dialog .sheets-format'); if (s) { s.value = 'xlsx'; s.dispatchEvent(new Event('change', { bubbles: true })); } return '';",
    )
    .await;
    pause(300).await;
    crate::commands::table_export::answer_save(xlsx.clone());
    let _ = text("document.querySelector('.dialog-foot .tbtn.primary')?.click(); return '';").await;
    let tables = st.read_active(|t| t.doc.page_tables(0..1)).unwrap_or_default();
    crate::commands::table_export::write_export(
        st,
        folitecta_core::TableFormat::Csv,
        tables,
        csv_out.clone(),
    );
    for _ in 0..40 {
        pause(150).await;
        if xlsx.exists() && csv_out.exists() {
            break;
        }
    }
    let parts = std::fs::read(&xlsx)
        .ok()
        .and_then(|d| folitecta_core::zip::read(&d).ok())
        .unwrap_or_default();
    let part = |name: &str| {
        parts
            .iter()
            .find(|p| p.name == name)
            .map(|p| String::from_utf8_lossy(&p.data).to_string())
            .unwrap_or_default()
    };
    let (book, sheet) = (part("xl/workbook.xml"), part("xl/worksheets/sheet1.xml"));
    let written = std::fs::read_to_string(&csv_out).unwrap_or_default();
    let unchanged = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(200).await;
    let page_name = crate::i18n::tf(st.lang(), "sheets.sheet", &[("page", "1")]);
    cx.record(
        "export tables: the table of the page listed with its size; a workbook through the dialog with numbers and the sheet named by its page, CSV as the cells read; the document unchanged",
        items.split('|').count() == 1
            && items.contains("4 × 3")
            && items.contains("Fruit")
            && book.contains(&format!("name=\"{page_name}\""))
            && sheet.contains("<v>1234.5</v>")
            && sheet.contains("<v>0.085</v>")
            && written.starts_with("\"Fruit\",\"Price\",\"Share\"\n\"Apples\",\"1,234.50\",\"12%\"")
            && unchanged,
        format!("items {items:?}, workbook {} bytes, sheet {} bytes, csv {written:?}, unchanged {unchanged}", book.len(), sheet.len()),
    );
}
