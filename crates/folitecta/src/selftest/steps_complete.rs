//! Interaction step: complete — the commands that finish the feature set:
//! group commands open the list of their group's commands, and a row runs
//! its command; Search › Annotate Result and the search options; the page
//! boxes and guides in the loupe; Interactive Visible of the thumbnails; the
//! options windows of panes and of recovery; Bates numbering of several
//! files from their list; the preview of page marks and its zoom.

use super::*;

/// The command ids of the rows of the open group window.
async fn rows() -> Vec<String> {
    eval("return [...document.querySelectorAll('.dialog .command-group-row')].map(b => b.dataset.cmd);")
        .await
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::text_doc(2), None, None)
    else {
        cx.record("complete", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;

    // Other Tools lists the tools; a click on the loupe chooses it.
    st.set_tool(Tool::Hand);
    crate::commands::dispatch(st, "otherToolsGroup");
    pause(400).await;
    let tools = rows().await;
    let _ =
        eval("document.querySelector('.dialog .command-group-row[data-cmd=\"tool.loupe\"]')?.click();").await;
    pause(400).await;
    let loupe = (st.tool)() == Tool::Loupe;
    let closed = matches!(*st.dialog.peek(), Dialog::None);
    st.set_tool(Tool::Hand);
    // Every group command lists commands that work, none planned.
    let groups = [
        "clipboardGroup",
        "selectionGroup",
        "view.snapGroup",
        "webLinksGroup",
        "shareGroup",
        "view.panesGroup",
        "view.otherPanes",
        "view.otherToolsGroup",
        "comment.otherToolsGroup",
        "view.pageLayoutTypesGroup",
        "view.rotateViewGroup",
        "view.docTabsGroup",
        "view.show",
        "tools.basic",
        "tools.markup",
        "tools.editContent",
        "tools.editLinks",
        "tools.measure",
        "comments.newGroup",
        "comments.deleteGroup",
        "document.pagesMore",
    ];
    let mut empty = Vec::new();
    for g in groups {
        crate::commands::dispatch(st, g);
        pause(200).await;
        let r = rows().await;
        let badges = eval("return document.querySelectorAll('.dialog .command-group .badge').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if r.is_empty() || badges > 0 {
            empty.push(g);
        }
        st.set_dialog(Dialog::None);
        pause(100).await;
    }
    // The rotate group turns the view.
    crate::commands::dispatch(st, "view.rotateViewGroup");
    pause(300).await;
    let _ =
        eval("document.querySelector('.dialog .command-group-row[data-cmd=\"view.rotate.CW\"]')?.click();")
            .await;
    pause(300).await;
    let turned = st.read_active(|t| t.view_rotation).unwrap_or(0);
    crate::commands::dispatch(st, "view.rotate.reset");
    cx.record(
        "group commands list their group's commands; a row runs its command and closes the window",
        tools.contains(&"tool.loupe".to_string()) && loupe && closed && empty.is_empty() && turned == 90,
        format!("other tools {tools:?}, loupe {loupe}, closed {closed}, groups empty or planned {empty:?}, view turned {turned}"),
    );

    search_and_loupe(cx).await;
    thumbnails_and_options(cx).await;
    bates_files(cx).await;
    mark_preview(cx).await;

    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
}

/// Annotate Result marks the current result only; the search toggles share
/// Find's options; the loupe shows the trim box and the guides.
async fn search_and_loupe(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let count = |st: AppState| {
        st.read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
            .unwrap_or(0)
    };
    let before = count(st);
    crate::ui::panes::run_search(st, "document");
    pause(300).await;
    let hits = st.read_active(|t| t.search.hits.len()).unwrap_or(0);
    crate::commands::dispatch(st, "search.select.annotateResult");
    pause(300).await;
    let marked = count(st) - before;
    let was = crate::commands::option_on(st, "find.annotateResults");
    crate::commands::dispatch(st, "search.annotateResults");
    let shared = crate::commands::option_on(st, "find.annotateResults") != was
        && crate::commands::toggle_state(st, "search.annotateResults") == Some(!was);
    crate::commands::dispatch(st, "search.annotateResults");
    crate::commands::dispatch(st, "search.stop");
    cx.record(
        "Search › Annotate Result highlights the current result only; Annotate Results is Find's option",
        hits > 1 && marked == 1 && shared,
        format!("results {hits}, comments added {marked}, option shared {shared}"),
    );

    // A trim box and a guide on page 1, both shown by the loupe.
    st.with_active(|t| {
        let _ = t.doc.set_page_box(
            &[0],
            folitecta_core::pages::PageBox::Trim,
            Some([30.0, 30.0, 30.0, 30.0]),
        );
        t.guides.push(crate::state::Guide {
            page: None,
            horizontal: true,
            pos: 120.0,
        });
    });
    st.set_option("loupeView.showBoxes", true);
    st.set_option("loupeView.showGuides", true);
    st.set_tool(Tool::Loupe);
    st.set_loupe(Some((0, 40.0, 120.0, 400.0, 300.0)));
    pause(500).await;
    let shown = eval("return [document.querySelectorAll('.loupe .page-box.trim').length, document.querySelectorAll('.loupe .guide.h').length];")
        .await
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<u64>>(v).ok())
        .unwrap_or_default();
    st.set_option("loupeView.showBoxes", false);
    pause(300).await;
    let hidden = eval("return document.querySelectorAll('.loupe .page-box').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(9);
    st.set_option("loupeView.showGuides", false);
    st.set_loupe(None);
    st.set_tool(Tool::Hand);
    st.with_active(|t| t.guides.clear());
    cx.record(
        "Loupe › Show Boxes and Show Guides draw the trim box and the guides in the loupe",
        shown == vec![1, 1] && hidden == 0,
        format!("trim box and guides shown {shown:?}, boxes after Show Boxes off {hidden}"),
    );
}

/// Interactive Visible: a press low on the current thumbnail scrolls the
/// view there; the options windows set their values.
async fn thumbnails_and_options(cx: &mut Ctx<'_>) {
    let st = cx.st;
    crate::commands::show_pane(st, Pane::Thumbnails);
    st.with_active(|t| t.zoom = crate::state::Zoom::Fixed(3.0));
    st.go_to_page(0);
    pause(800).await;
    st.set_option("thumbnailsView.interactiveVisible", true);
    pause(500).await;
    let top = || eval("return document.getElementById('canvas')?.scrollTop ?? -1;");
    let before = top().await.ok().and_then(|v| v.as_f64()).unwrap_or(-1.0);
    let _ = eval("const i = document.querySelector('.thumb.current .thumb-img.interactive'); if (i) { const r = i.getBoundingClientRect(); i.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height * 0.8 })); i.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height * 0.8 })); }").await;
    pause(600).await;
    let after = top().await.ok().and_then(|v| v.as_f64()).unwrap_or(-1.0);
    st.set_option("thumbnailsView.interactiveVisible", false);
    st.with_active(|t| t.zoom = crate::state::Zoom::FitWidth);
    cx.record(
        "Thumbnails › Interactive Visible: a press low on the current page's thumbnail scrolls the view down",
        before >= 0.0 && after > before + 100.0,
        format!("canvas scrolled from {before} to {after}"),
    );

    // Recovery › Options keeps copies every 5 minutes; Thumbnails › Options
    // turns Logical Visible on with its check box.
    crate::commands::dispatch(st, "recoveryView.options");
    pause(300).await;
    let _ = eval("const s = document.querySelector('.dialog select.recovery-every'); if (s) { s.value = '5'; s.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(200).await;
    let minutes = crate::recovery::minutes(st);
    st.set_dialog(Dialog::None);
    let was = crate::commands::option_on(st, "thumbnailsView.logicalVisible");
    crate::commands::dispatch(st, "thumbnailsView.options");
    pause(300).await;
    let _ =
        eval("document.querySelector('.dialog input[data-opt=\"thumbnailsView.logicalVisible\"]')?.click();")
            .await;
    pause(200).await;
    let toggled = crate::commands::option_on(st, "thumbnailsView.logicalVisible") != was;
    st.set_option("thumbnailsView.logicalVisible", was);
    st.set_value("recovery.minutes", "1");
    st.set_dialog(Dialog::None);
    cx.record(
        "Recovery › Options and Thumbnails › Options set their values",
        minutes == 5 && toggled,
        format!("recovery every {minutes} minutes, logical visible toggled {toggled}"),
    );
}

/// Two files in the list, the second moved first, the first given one page:
/// the copies get the numbers in that order; a column shown and hidden.
async fn bates_files(cx: &mut Ctx<'_>) {
    use crate::commands::bates_files::BATES_FILES;
    let st = cx.st;
    let dir = std::env::temp_dir().join(format!("foli-bates-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (a, b) = (dir.join("a.pdf"), dir.join("b.pdf"));
    let _ = std::fs::write(&a, folitecta_core::testdocs::text_doc(2));
    let _ = std::fs::write(&b, folitecta_core::testdocs::text_doc(3));
    *BATES_FILES.write() = Default::default();
    crate::commands::dispatch(st, "document.batesNumbering.addMulti");
    crate::commands::bates_files::add_paths(vec![a.clone(), b.clone()]);
    for _ in 0..30 {
        if BATES_FILES.peek().items.len() == 2 {
            break;
        }
        pause(100).await;
    }
    pause(300).await;
    let heads = || eval("return document.querySelectorAll('.dialog table.bates-files th').length;");
    let before = heads().await.ok().and_then(|v| v.as_u64()).unwrap_or(0);
    crate::commands::dispatch(st, "batesNumberingFiles.hdr.size");
    pause(300).await;
    let more = heads().await.ok().and_then(|v| v.as_u64()).unwrap_or(0);
    crate::commands::dispatch(st, "batesNumberingFiles.hdr.size");
    // b first, a with its first page only.
    let _ = eval("document.querySelectorAll('.dialog table.bates-files tbody tr')[1]?.click();").await;
    pause(200).await;
    crate::commands::dispatch(st, "list.up");
    pause(200).await;
    let order: Vec<String> = BATES_FILES
        .peek()
        .items
        .iter()
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    let _ = eval("const i = document.getElementById('bates-range-1'); if (i) { i.value = '1'; i.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    let _ = eval("const f = document.querySelectorAll('.dialog .field-grid input'); if (f[0]) { f[0].value = 'B{page}'; f[0].dispatchEvent(new Event('input', { bubbles: true })); } if (f[1]) { f[1].value = '7'; f[1].dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let (ab, bb) = (dir.join("a-bates.pdf"), dir.join("b-bates.pdf"));
    for _ in 0..50 {
        if ab.exists() && bb.exists() {
            break;
        }
        pause(200).await;
    }
    pause(300).await;
    let text = |p: &std::path::Path, page: usize| {
        folitecta_core::Document::open(p)
            .map(|d| d.page_text(page))
            .unwrap_or_default()
    };
    let (b1, b3, a1, a2) = (text(&bb, 0), text(&bb, 2), text(&ab, 0), text(&ab, 1));
    let numbered = b1.contains("B7") && b3.contains("B9") && a1.contains("B10") && !a2.contains("B11");
    let next = st.value("bates.next");
    let _ = std::fs::remove_dir_all(&dir);
    st.set_value("bates.next", "1");
    st.set_value("bates.format", "");
    *BATES_FILES.write() = Default::default();
    cx.record(
        "Bates numbering of several files: the list ordered, pages chosen, a column added; the copies numbered in that order",
        more == before + 1 && order == ["b.pdf", "a.pdf"] && numbered && next == "11",
        format!("columns {before} then {more}, order {order:?}, numbered {numbered}, next {next}"),
    );
}

/// The watermark window shows the page with the watermark; Zoom In draws it
/// larger.
async fn mark_preview(cx: &mut Ctx<'_>) {
    let st = cx.st;
    st.set_value("marks.preview_zoom", "0.5");
    let bytes = st.read_active(|t| t.doc.bytes());
    st.set_dialog(Dialog::Watermark);
    let width = || {
        eval(
            "const i = document.querySelector('.dialog .mark-preview-page img'); return i && i.complete ? i.naturalWidth : 0;",
        )
    };
    let mut first = 0;
    for _ in 0..30 {
        pause(200).await;
        first = width().await.ok().and_then(|v| v.as_u64()).unwrap_or(0);
        if first > 0 {
            break;
        }
    }
    crate::commands::dispatch(st, "pagemarks.zoomIn");
    let mut second = 0;
    for _ in 0..30 {
        pause(200).await;
        second = width().await.ok().and_then(|v| v.as_u64()).unwrap_or(0);
        if second > first {
            break;
        }
    }
    let changed = match (bytes, st.read_active(|t| t.doc.bytes())) {
        (Some(a), Some(b)) => !std::sync::Arc::ptr_eq(&a, &b),
        _ => true,
    };
    st.set_dialog(Dialog::None);
    cx.record(
        "the watermark window previews the page with the mark; Page Marks › Zoom In draws it larger; the document is not changed",
        first > 250 && first < 350 && second > first && !changed,
        format!("preview {first} px wide, after Zoom In {second} px, document changed {changed}"),
    );
}
