//! Interaction step: csv_options — a CSV file opened as a table is made again
//! from Table Options with the pages turned sideways and Letter paper, in the
//! same tab; a document not made from CSV has no options.

use super::*;

async fn choose(class: &str, value: &str) {
    let _ = eval(&format!(
        "const s = document.querySelector('.dialog .{class}'); if (s) {{ s.value = '{value}'; s.dispatchEvent(new Event('change', {{ bubbles: true }})); }}"
    ))
    .await;
    pause(150).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let path = cx.dir.join("ledger-options.csv");
    if std::fs::write(&path, "Name;Amount\nRossi;1234,50\nBianchi;20,00\n").is_err() {
        cx.record("table options: the file", false, String::new());
        return;
    }
    let tabs = st.docs.read().len();
    crate::files::open_path(st, path, None);
    pause(600).await;
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    crate::commands::dispatch(st, "csv.tableOptions");
    pause(400).await;
    let window = matches!(*st.dialog.peek(), Dialog::CsvOptions);
    choose("csv-turn", "sideways").await;
    choose("csv-paper", "Letter").await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let (same_tab, size, text) = st
        .read_active(|t| (t.id == tab, t.doc.page_size(0).ok(), t.doc.page_text(0)))
        .unwrap_or_default();
    let turned = size.is_some_and(|s| (s.width - 792.0).abs() < 1.0 && (s.height - 612.0).abs() < 1.0);
    let one_tab = st.docs.read().len() == tabs + 1;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "table options: a CSV table made again sideways on Letter paper, in its tab",
        window && same_tab && turned && one_tab && text.contains("Bianchi"),
        format!("window {window}, same tab {same_tab}, size {size:?}, one tab {one_tab}, text {text:?}"),
    );
}
