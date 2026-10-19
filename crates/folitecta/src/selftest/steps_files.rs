//! Interaction step 41: the explorer pane and the source files of a combined document.

use std::collections::BTreeSet;

use super::*;
use crate::commands::explorer::{
    EXPLORER, current_dir, custom_filters, custom_places, filter_index, go, remove_place,
};
use crate::commands::sources::{SOURCES, Sources, add_paths};
use crate::state::ConfirmAction;

async fn count(selector: &str) -> u64 {
    eval(&format!("return document.querySelectorAll('{selector}').length;"))
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Type `values` into the inputs of the open dialog, in order.
async fn fill_dialog(values: &[&str]) {
    let list = serde_json::to_string(values).unwrap_or_default();
    let js = format!(
        "const v = {list}; const ins = document.querySelectorAll('.dialog input'); v.forEach((x, i) => {{ const el = ins[i]; if (el) {{ el.value = x; el.dispatchEvent(new Event('input', {{ bubbles: true }})); }} }});"
    );
    let _ = eval(&js).await;
    pause(200).await;
}

async fn press_ok() {
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    // 41. Explorer: a folder with a PDF file, a picture, a text file and
    // another file, shown through the filters, a thumbnail, a new folder
    // entered and left, a place, a custom filter, and deleting that asks
    // first; then the files combined into a new document.
    let folder = dir.join("explorer-files");
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::create_dir_all(&folder);
    let lines: String = (1..=140).map(|i| format!("Line {i} of the report\n")).collect();
    let pdf = folder.join("report.pdf");
    let pdf_pages = match folitecta_core::Document::from_text(&lines, folitecta_core::TextLayout::default()) {
        Ok(doc) => {
            let _ = std::fs::write(&pdf, doc.bytes().as_slice());
            doc.page_count()
        }
        Err(_) => 0,
    };
    let png = folder.join("picture.png");
    let _ = image::RgbaImage::from_pixel(80, 60, image::Rgba([200, 60, 60, 255])).save(&png);
    let txt = folder.join("notes.txt");
    let _ = std::fs::write(&txt, "Notes for the combined document.\n");
    let other = folder.join("data.bin");
    let _ = std::fs::write(&other, [0u8, 1, 2, 3]);

    crate::commands::dispatch(st, "explorer");
    go(st, folder.clone());
    let mut shown = Vec::new();
    for f in ["0", "1", "2"] {
        st.set_value("explorer.filter", f);
        pause(500).await;
        shown.push(count(".explorer-list .explorer-row").await);
    }
    cx.record(
        "explorer lists the folder through its filters",
        shown == [1, 3, 4] && current_dir(st) == folder,
        format!("{shown:?} rows, {pdf_pages} pages in the PDF file"),
    );
    st.set_value("explorer.filter", "1");
    let mut thumb = false;
    for _ in 0..40 {
        pause(150).await;
        thumb = eval("const i = document.querySelector('img.explorer-thumb'); return !!(i && i.complete && i.naturalWidth > 0);")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if thumb {
            break;
        }
    }
    cx.record("explorer shows a thumbnail of the PDF file", thumb, String::new());
    let _ = capture_window(&dir.join("interaction-explorer.png"));

    crate::commands::dispatch(st, "explorer.folder.create");
    pause(400).await;
    fill_dialog(&["Drafts"]).await;
    press_ok().await;
    let drafts = folder.join("Drafts");
    let _ = eval(
        "const r = [...document.querySelectorAll('.explorer-list .explorer-row')].find(e => e.textContent.includes('Drafts')); if (r) r.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));",
    )
    .await;
    pause(400).await;
    let inside = current_dir(st) == drafts;
    crate::commands::dispatch(st, "explorer.folder.back");
    let back = current_dir(st) == folder;
    crate::commands::dispatch(st, "explorer.folder.forward");
    let forward = current_dir(st) == drafts;
    crate::commands::dispatch(st, "explorer.folder.up");
    let up = current_dir(st) == folder;
    cx.record(
        "explorer creates a folder, enters it and goes back",
        drafts.is_dir() && inside && back && forward && up,
        format!("inside {inside}, back {back}, forward {forward}, up {up}"),
    );

    crate::commands::dispatch(st, "addPlace");
    pause(400).await;
    let listed = eval(
        "return [...document.querySelectorAll('.explorer-places .tree-item')].some(e => e.textContent.includes('explorer-files'));",
    )
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    cx.record(
        "explorer keeps the folder among the places",
        custom_places(st).contains(&folder) && listed,
        String::new(),
    );
    remove_place(st, &folder);

    st.set_dialog(Dialog::FolderFilter(None));
    pause(400).await;
    fill_dialog(&["Pictures", "png"]).await;
    press_ok().await;
    let rows = count(".explorer-list .explorer-row").await;
    let custom = filter_index(st) == 3 && custom_filters(st).first().is_some_and(|f| f.exts == ["png"]);
    // The picture and the new folder.
    cx.record(
        "custom folder filter",
        custom && rows == 2,
        format!("{rows} rows"),
    );
    crate::commands::dispatch(st, "folderExts.removeCustom");
    let removed = custom_filters(st).is_empty() && filter_index(st) == 0;

    EXPLORER.write().selected = BTreeSet::from([drafts.clone()]);
    crate::commands::dispatch(st, "explorer.delete");
    pause(300).await;
    let asks = matches!(
        *st.dialog.read(),
        Dialog::Confirm {
            action: ConfirmAction::TrashFiles,
            ..
        }
    );
    st.set_dialog(Dialog::None);
    cx.record(
        "custom filter removed, deleting files asks first",
        removed && asks && drafts.is_dir(),
        String::new(),
    );

    // Source files: the other file is left out, the picture moved first and
    // only the second page of the PDF file taken.
    *SOURCES.write() = Sources::default();
    add_paths(st, vec![pdf.clone(), png.clone(), txt.clone(), other.clone()]);
    st.set_dialog(Dialog::Combine);
    pause(500).await;
    let listed = count(".source-list .source-row:not(.head)").await;
    let _ = eval("document.querySelectorAll('.source-list .source-row:not(.head)')[1]?.click();").await;
    pause(200).await;
    crate::commands::dispatch(st, "newDocumentList.top");
    pause(300).await;
    let _ = eval(
        "const i = document.getElementById('source-range-1'); if (i) { i.value = '2'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    crate::commands::dispatch(st, "newDocumentList.viewMode");
    pause(400).await;
    let order: Vec<PathBuf> = SOURCES.peek().items.iter().map(|i| i.path.clone()).collect();
    let range = SOURCES
        .peek()
        .items
        .get(1)
        .map(|i| i.range.clone())
        .unwrap_or_default();
    let _ = capture_window(&dir.join("interaction-combine.png"));
    cx.record(
        "source files listed and moved",
        listed == 3 && order == [png.clone(), pdf.clone(), txt.clone()] && range == "2",
        format!("{listed} rows, range {range:?}"),
    );
    let tabs = st.docs.read().len();
    press_ok().await;
    pause(500).await;
    let combined = st
        .read_active(|t| (t.doc.path().is_none(), t.doc.page_count()))
        .unwrap_or((false, 0));
    cx.record(
        "files combined into a new document",
        st.docs.read().len() == tabs + 1 && combined == (true, 3),
        format!("{combined:?}"),
    );
    if st.docs.read().len() == tabs + 1
        && let Some(i) = st.active_index()
    {
        st.docs_mut().remove(i);
        st.set_active(0);
    }
    st.set_dialog(Dialog::None);
    *SOURCES.write() = Sources::default();
    *EXPLORER.write() = Default::default();
    st.ui_mut().left_pane = "thumbnails".into();
    pause(400).await;
}
