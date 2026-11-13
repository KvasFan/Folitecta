//! Interaction step: complete — the commands that finish the feature set:
//! group commands open the list of their group's commands, and a row runs
//! its command; Search › Annotate Result and the search options; the page
//! boxes and guides in the loupe; Interactive Visible of the thumbnails; the
//! options windows of panes and of recovery; Bates numbering of several
//! files from their list; the preview of page marks and its zoom; the
//! Presets window; toolbars and tabs of one's own, locked toolbars and panes;
//! guides and form fields copied and pasted, files pasted, the file copied;
//! comment labels and text, a comment on selected text, measurement
//! information, style lists, rows of data in page marks; documents moved
//! between windows, a group of tabs of their own.

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
    presets(cx).await;
    mail_method(cx).await;
    own_bars(cx).await;
    clipboard_more(cx).await;
    comments_more(cx).await;
    windows(cx).await;

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
    // The panes as they were, for the steps after this one.
    let (shown, pane) = {
        let ui = st.ui.peek();
        (ui.show_left_pane, ui.left_pane.clone())
    };
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
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = shown;
        ui.left_pane = pane;
    }
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
    st.unset_value("recovery.minutes");
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
    st.unset_value("bates.next");
    st.unset_value("bates.format");
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
    st.unset_value("marks.preview_zoom");
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

/// Measurement presets: added from the settings in use and from the
/// defaults, renamed in place, moved, cloned, deleted, used; imported from
/// a file of them; Digital Signatures › Reset.
async fn presets(cx: &mut Ctx<'_>) {
    use crate::commands::presets::{CHOSEN, load};
    let st = cx.st;
    st.unset_value("measure.presets");
    st.set_value("measure.decimals", "3");
    crate::commands::dispatch(st, "measures.manage");
    pause(300).await;
    let open = matches!(*st.dialog.peek(), Dialog::Presets) && CHOSEN.peek().kind == "measure";
    crate::commands::dispatch(st, "presetList.add");
    st.set_value("measure.decimals", "1");
    crate::commands::dispatch(st, "presets.addNew2");
    pause(300).await;
    let _ = eval("const i = document.getElementById('preset-name-0'); if (i) { i.value = 'Fine'; i.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(300).await;
    // The defaults first, then Fine; a clone of Fine added and deleted.
    CHOSEN.write().index = Some(1);
    crate::commands::dispatch(st, "presetList.up");
    crate::commands::dispatch(st, "presetList.down");
    crate::commands::dispatch(st, "presetList.clone");
    let three = load(st, "measure").len();
    crate::commands::dispatch(st, "presetList.delete");
    pause(300).await;
    let names: Vec<String> = load(st, "measure").into_iter().map(|p| p.0).collect();
    // Use Fine.
    let at = names.iter().position(|n| n == "Fine");
    CHOSEN.write().index = at;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog .preset-use')?.click();").await;
    pause(300).await;
    let used = st.value("measure.decimals");
    let imported = crate::commands::presets::import_text(
        st,
        r#"{"folitecta_presets":"measure","presets":[["Fine",{"measure.decimals":"2","other":"x"}]]}"#,
    );
    let after = load(st, "measure");
    let kept_only_known = after
        .last()
        .is_some_and(|p| p.0 == "Fine (2)" && !p.1.contains_key("other"));
    st.set_value("sign.reason", "Approved");
    crate::commands::dispatch(st, "digiSigList.reset");
    let reset = st.value("sign.reason").is_empty();
    st.set_dialog(Dialog::None);
    st.unset_value("measure.presets");
    st.unset_value("measure.decimals");
    cx.record(
        "the Presets window adds, renames, moves, clones, deletes, uses and imports measurement presets; Digital Signatures › Reset",
        open && three == 3 && names.len() == 2 && names.contains(&"Fine".to_string()) && used == "3"
            && imported == Ok(1) && kept_only_known && reset,
        format!("open {open}, after clone {three}, names {names:?}, decimals used {used}, imported {imported:?} known keys only {kept_only_known}, sign reset {reset}"),
    );
}

/// Email Methods › Manage: a draft first; the e-mail window's main button
/// saves the draft, the mail program a button of its own.
async fn mail_method(cx: &mut Ctx<'_>) {
    let st = cx.st;
    crate::commands::dispatch(st, "emailMethods.manage");
    pause(300).await;
    let _ = eval("document.querySelector('.dialog input[name=mail-method][value=draft]')?.click();").await;
    pause(200).await;
    let chosen = st.value("mail.method");
    st.set_dialog(Dialog::None);
    crate::commands::dispatch(st, "sendByEmail");
    pause(600).await;
    let buttons = eval("return [document.querySelector('.dialog-foot .tbtn.primary')?.textContent ?? '', !!document.querySelector('.dialog .mail-program')];")
        .await
        .ok()
        .unwrap_or_default();
    let main = buttons
        .get(0)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let program = buttons.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
    st.set_dialog(Dialog::None);
    *crate::commands::mail::MAIL.write() = None;
    st.unset_value("mail.method");
    let want = crate::i18n::t(st.lang(), "dialog.mail.save_draft");
    cx.record(
        "Email Methods › Manage: with a draft first the e-mail window's main button saves the draft",
        chosen == "draft" && main.trim() == want && program,
        format!("method {chosen}, main button {main:?}, mail program button {program}"),
    );
}

/// A toolbar and a ribbon tab of one's own, each with a tool; the tab
/// shown in the ribbon; renamed; the toolbars locked refuse changes; the
/// panes locked keep their width.
async fn own_bars(cx: &mut Ctx<'_>) {
    use crate::commands::qat;
    let st = cx.st;
    st.unset_value("user.bars");
    let qat_before = qat::items(st);
    crate::commands::dispatch(st, "customizeToolbars.newToolbar");
    pause(200).await;
    qat::add(st, "zoomIn");
    crate::commands::dispatch(st, "customizeToolbars.newTab");
    pause(200).await;
    qat::add(st, "zoomOut");
    qat::set_name(st, "Review");
    let bars = qat::user_bars(st);
    let made = bars.len() == 2
        && !bars[0].tab
        && bars[0].items.iter().map(|i| i.id.as_str()).eq(["zoomIn"])
        && bars[1].tab
        && bars[1].name == "Review"
        && bars[1].items.iter().map(|i| i.id.as_str()).eq(["zoomOut"]);
    let qat_kept = qat::items(st) == qat_before;
    st.set_dialog(Dialog::None);
    let ribbon = st.ui.peek().ribbon;
    st.ui_mut().ribbon = true;
    st.set_ribbon_tab("user:1".into());
    pause(500).await;
    let shown = eval("return [!!document.querySelector('.ribbon-tab.user.active'), !!document.querySelector('.ribbon-body.user-tab [data-cmd=\"zoomOut\"]')];")
        .await
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<bool>>(v).ok())
        .unwrap_or_default();
    st.set_ribbon_tab("home".into());
    // Locked toolbars refuse a new tool; locked panes keep their width.
    crate::commands::dispatch(st, "view.lockAllCmdBars");
    let locked = crate::commands::toggle_state(st, "view.lockAllCmdBars") == Some(true)
        && crate::commands::toggle_state(st, "view.unlockAllCmdBars") == Some(false);
    *qat::TARGET.write() = Some(0);
    qat::add(st, "zoom.level.actual");
    let refused = qat::user_bars(st)[0].items.len() == 1;
    crate::commands::dispatch(st, "view.unlockAllCmdBars");
    crate::commands::dispatch(st, "view.lockAllCmdPanes");
    let panes = crate::ui::chrome::layout::locked(st, crate::ui::chrome::layout::LEFT_W)
        && !crate::ui::chrome::layout::locked(st, crate::ui::chrome::layout::RIBBON_ROWS);
    crate::commands::dispatch(st, "view.lockUnlockAllCmdPanes");
    let unlocked = !crate::ui::chrome::layout::locked(st, crate::ui::chrome::layout::LEFT_W);
    st.ui_mut().ribbon = ribbon;
    st.unset_value("user.bars");
    *qat::TARGET.write() = None;
    cx.record(
        "toolbars and ribbon tabs of one's own get their tools, the tab shows them; locked toolbars refuse changes, locked panes their handles",
        made && qat_kept && shown == vec![true, true] && locked && refused && panes && unlocked,
        format!("made {made}, quick access kept {qat_kept}, tab shown {shown:?}, locked {locked}, refused {refused}, panes {panes}, unlocked {unlocked}"),
    );
}

/// Guides chosen on a page, copied and pasted on another; deleted. A form
/// field copied into another document. A PDF file on the system clipboard
/// pasted as pages; Copy File puts the document's file there.
async fn clipboard_more(cx: &mut Ctx<'_>) {
    use crate::state::{Clip, Guide};
    let st = cx.st;
    st.with_active(|t| {
        t.guides = vec![
            Guide {
                page: Some(0),
                horizontal: true,
                pos: 100.0,
            },
            Guide {
                page: Some(1),
                horizontal: true,
                pos: 300.0,
            },
            Guide {
                page: None,
                horizontal: false,
                pos: 50.0,
            },
        ];
        t.guide_sel.clear();
    });
    let guides_shown = crate::commands::option_on(st, "view.showGuides");
    st.set_option("view.showGuides", true);
    st.go_to_page(0);
    pause(300).await;
    crate::commands::dispatch(st, "guidesSelectOnPage");
    let chosen = st.read_active(|t| t.guide_sel.clone()).unwrap_or_default();
    pause(300).await;
    let drawn = eval("return document.querySelectorAll('#pg-0 .guide.chosen').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    crate::commands::dispatch(st, "copy");
    let copied = matches!(st.clip.peek().as_ref(), Some(Clip::Guides(g)) if g.len() == 2);
    st.go_to_page(1);
    pause(200).await;
    crate::commands::dispatch(st, "pasteGuides");
    let on_second = st
        .read_active(|t| t.guides.iter().filter(|g| g.page == Some(1)).count())
        .unwrap_or(0);
    crate::commands::dispatch(st, "guidesSelectAll");
    crate::commands::dispatch(st, "delete");
    let left = st.read_active(|t| t.guides.len()).unwrap_or(9);
    st.set_option("view.showGuides", guides_shown);
    cx.record(
        "Guides › Select on Page chooses the page's guides, Copy and Paste Guides put them on another page, Delete takes the chosen ones",
        chosen == vec![0, 2] && drawn == 2 && copied && on_second == 2 && left == 0,
        format!("chosen {chosen:?}, drawn chosen {drawn}, copied {copied}, on page 2 {on_second}, left {left}"),
    );

    // A text field copied with the form tool, pasted into a new document.
    let props = folitecta_core::FieldProps {
        name: "Town".into(),
        ..Default::default()
    };
    let field = st.with_active(|t| {
        t.doc.add_form_field(
            0,
            folitecta_core::NewField::Text,
            [100.0, 600.0, 250.0, 620.0],
            &props,
        )
    });
    let Some(Ok(field)) = field else {
        cx.record("form fields copied", false, "the field could not be made".into());
        return;
    };
    let from = st.read_active(|t| t.id).unwrap_or(0);
    st.set_tool(Tool::EditFields);
    st.with_active(|t| t.selected_fields = vec![field]);
    crate::commands::dispatch(st, "copy");
    let fields_copied =
        matches!(st.clip.peek().as_ref(), Some(Clip::Fields { fields, .. }) if fields.len() == 1);
    st.set_tool(Tool::Hand);
    let Ok(other) = folitecta_core::Document::new_blank(1, 595.0, 842.0) else {
        return;
    };
    crate::files::open_document_tab(st, other);
    pause(400).await;
    crate::commands::dispatch(st, "pasteFields");
    let pasted: Vec<String> = st
        .read_active(|t| t.doc.form_fields().into_iter().map(|f| f.name).collect())
        .unwrap_or_default();
    let other_tab = st.read_active(|t| t.id).unwrap_or(0);

    // Files on the system clipboard.
    let dir = std::env::temp_dir().join(format!("foli-clip-files-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let pdf = dir.join("two.pdf");
    let _ = std::fs::write(&pdf, folitecta_core::testdocs::text_doc(2));
    let set = crate::clipboard::set_files(std::slice::from_ref(&pdf));
    pause(300).await;
    let back = crate::clipboard::get_files();
    crate::commands::dispatch(st, "pasteFiles");
    pause(300).await;
    let pages = st.read_active(|t| t.doc.page_count()).unwrap_or(0);
    // Copy File: the document saved as a file, then put on the clipboard.
    let saved = dir.join("saved.pdf");
    let wrote = st
        .with_active(|t| t.doc.save_as(&saved))
        .is_some_and(|r| r.is_ok());
    crate::commands::dispatch(st, "copySrcFile");
    pause(300).await;
    let file_on_clip = crate::clipboard::get_files();
    crate::files::close_tab(st, other_tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let _ = std::fs::remove_dir_all(&dir);
    st.set_clip(None);
    let back_to = st.read_active(|t| t.id).unwrap_or(0) == from;
    cx.record(
        "a form field copied with the form tool is pasted into another document; files on the clipboard become pages; Copy File puts the file there",
        fields_copied && pasted == vec!["Town".to_string()] && set.is_ok() && back == vec![pdf.clone()] && pages == 3
            && wrote && file_on_clip == vec![saved.clone()] && back_to,
        format!("copied {fields_copied}, pasted {pasted:?}, clipboard set {set:?} read {back:?}, pages {pages}, saved {wrote}, file on clipboard {file_on_clip:?}"),
    );
}

async fn comments_more(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let lang = st.lang();
    // A note given a label, then without it; Add Text opens its text.
    let made = st.with_active(|t| t.doc.add_note(0, 100.0, 700.0, "", "Reviewer", [1.0, 0.8, 0.0]));
    let index = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(1) - 1)
        .unwrap_or(0);
    st.with_active(|t| t.selected_annot = Some((0, index)));
    crate::commands::dispatch(st, "annot.editLabel");
    pause(300).await;
    let _ = eval("const i = document.querySelector('.dialog input'); if (i) { i.value = 'Check'; i.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(300).await;
    let subject = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .annotations(0)
                .ok()
                .and_then(|a| a.get(index).map(|x| x.subject.clone()))
        })
        .flatten()
        .unwrap_or_default()
    };
    let labelled = subject(st);
    crate::commands::dispatch(st, "annot.deleteLabel");
    let unlabelled = subject(st);
    crate::commands::dispatch(st, "annot.addText");
    pause(200).await;
    let text_window = matches!(*st.dialog.peek(), Dialog::AnnotationText { .. });
    st.set_dialog(Dialog::None);
    // A comment on selected text: a highlight, its note open.
    let before = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    st.with_active(|t| {
        t.text_sel = Some(crate::state::TextSel {
            page: 0,
            anchor: 0,
            head: 12,
        })
    });
    crate::commands::dispatch(st, "textsel.comment");
    pause(300).await;
    let after = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    let note_open = matches!(*st.dialog.peek(), Dialog::AnnotationText { .. });
    st.set_dialog(Dialog::None);
    // A distance measured: its information copied.
    let style = folitecta_core::AnnotStyle::default();
    let measured = st.with_active(|t| {
        t.doc.add_measurement(
            0,
            folitecta_core::MeasureKind::Distance,
            &[(100.0, 100.0), (172.0, 100.0)],
            &style,
            &folitecta_core::MeasureScale::default(),
            "Reviewer",
        )
    });
    let m_index = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(1) - 1)
        .unwrap_or(0);
    st.with_active(|t| t.selected_annot = Some((0, m_index)));
    crate::commands::dispatch(st, "measureInfo.copy");
    pause(300).await;
    let copied = crate::clipboard::get_text().unwrap_or_default();
    // The tool's styles list.
    st.set_tool(Tool::Rect);
    crate::commands::dispatch(st, "toolStylesList");
    pause(300).await;
    let style_rows = eval("return document.querySelectorAll('.dialog .style-row').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    st.set_dialog(Dialog::None);
    st.set_tool(Tool::Hand);
    // Rows of data from a CSV file and a region of one's own.
    let dir = std::env::temp_dir().join(format!("foli-rows-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join("rows.csv");
    let _ = std::fs::write(&csv, "Name,City\nAnna,Rome\nBruno,Turin\n");
    st.set_value("macro.rowdata.file", csv.display().to_string());
    st.set_value("macro.region", "IT");
    crate::commands::comment_more::apply_macro_data(st);
    let expanded = st
        .read_active(|t| {
            t.doc
                .expand_macros_numbered("{rowdata:City} {rowindex} {region}", 1, 2)
        })
        .unwrap_or_default();
    st.unset_value("macro.rowdata.file");
    st.unset_value("macro.region");
    crate::commands::comment_more::apply_macro_data(st);
    let _ = std::fs::remove_dir_all(&dir);
    let kind = crate::i18n::annot_name(lang, "Distance");
    cx.record(
        "comment labels set and taken away; Add Text; a comment on selected text; a measurement's information copied; the tool's styles; rows of data in page marks",
        made.is_some_and(|r| r.is_ok()) && labelled == "Check" && unlabelled.is_empty() && text_window && after == before + 1 && note_open
            && measured.is_some_and(|r| r.is_ok()) && copied.starts_with(&kind) && style_rows >= 1 && expanded == "Turin 2 IT",
        format!("label {labelled:?} then {unlabelled:?}, text window {text_window}, comments {before}->{after} note open {note_open}, copied {copied:?}, style rows {style_rows}, expanded {expanded:?}"),
    );
}

/// Another window stands for itself (its lock held, its note written): a
/// saved document moved to it, its documents asked for by Merge All Tabs, a
/// document arriving from it; Distribute Tabs; tabs in a group of their own.
async fn windows(cx: &mut Ctx<'_>) {
    use crate::commands::windows::{Message, check_now, others, send};
    let st = cx.st;
    let Some(rdir) = crate::recovery::folder() else {
        cx.record("windows", false, "no recovery folder".into());
        return;
    };
    let _ = std::fs::create_dir_all(&rdir);
    let fake = "selftest-other-window";
    let lock = std::fs::File::create(rdir.join(format!("{fake}.lock")));
    let held = lock.as_ref().is_ok_and(|f| f.try_lock().is_ok());
    let _ = std::fs::write(
        rdir.join(format!("{fake}.window")),
        br#"{"session":"","titles":["plan.pdf"]}"#,
    );
    let seen = others()
        .iter()
        .any(|w| w.session == fake && w.titles == ["plan.pdf"]);
    let dir = std::env::temp_dir().join(format!("foli-windows-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (a, b) = (dir.join("a.pdf"), dir.join("b.pdf"));
    let _ = std::fs::write(&a, folitecta_core::testdocs::text_doc(1));
    let _ = std::fs::write(&b, folitecta_core::testdocs::text_doc(1));
    let inbox = |rdir: &std::path::Path| -> Vec<Message> {
        std::fs::read_dir(rdir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{fake}.inbox."))
            })
            .filter_map(|e| {
                let m = std::fs::read(e.path())
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok());
                let _ = std::fs::remove_file(e.path());
                m
            })
            .collect()
    };
    let tabs = |st: AppState| st.docs.peek().len();
    // Move Tabs: a.pdf to the other window.
    let start = tabs(st);
    crate::files::open_path(st, a.clone(), None);
    pause(300).await;
    crate::commands::dispatch(st, "window.moveTabsTo");
    pause(300).await;
    let _ = eval(&format!(
        "document.querySelector('.dialog .move-to-window[data-session=\"{fake}\"]')?.click();"
    ))
    .await;
    pause(300).await;
    let moved = inbox(&rdir) == vec![Message::Open(vec![a.clone()])] && tabs(st) == start;
    // A document arriving from it, then Merge All Tabs asked by it.
    let me = crate::recovery::session().to_string();
    let _ = send(&me, &Message::Open(vec![b.clone()]));
    check_now(st);
    pause(300).await;
    let arrived = tabs(st) == start + 1;
    let _ = send(&me, &Message::Gather(fake.to_string()));
    check_now(st);
    pause(300).await;
    let gathered = inbox(&rdir) == vec![Message::Open(vec![b.clone()])] && tabs(st) == start;
    // Distribute Tabs: a.pdf to a window of its own (noted, not started).
    let noting = crate::commands::tab_groups::noting();
    crate::commands::tab_groups::set_noting(true);
    let step_tab = st.read_active(|t| t.id).unwrap_or(0);
    crate::files::open_path(st, a.clone(), None);
    pause(200).await;
    if let Some(i) = st.docs.peek().iter().position(|t| t.id == step_tab) {
        st.set_active(i);
    }
    crate::commands::dispatch(st, "window.distributeTabs");
    pause(300).await;
    let distributed = crate::commands::tab_groups::noted() == vec![vec![a.clone()]] && tabs(st) == start;
    crate::commands::tab_groups::set_noting(noting);
    // Tabs in a group of their own.
    crate::files::open_path(st, b.clone(), None);
    pause(200).await;
    let new_id = st.read_active(|t| t.id).unwrap_or(0);
    crate::commands::tab_groups::into_new_group(st, &[new_id]);
    pause(300).await;
    let grouped = st
        .groups
        .peek()
        .as_ref()
        .is_some_and(|g| g.tabs[1] == vec![new_id]);
    crate::files::close_tab(st, new_id, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    if let Some(i) = st.docs.peek().iter().position(|t| t.id == step_tab) {
        st.set_active(i);
    }
    if let Ok(f) = &lock {
        let _ = f.unlock();
    }
    for ext in ["lock", "window"] {
        let _ = std::fs::remove_file(rdir.join(format!("{fake}.{ext}")));
    }
    let _ = inbox(&rdir);
    let _ = std::fs::remove_dir_all(&dir);
    cx.record(
        "documents travel between windows: Move Tabs, a document arriving, Merge All Tabs asked by another window, Distribute Tabs; tabs in a group of their own",
        held && seen && moved && arrived && gathered && distributed && grouped,
        format!("lock held {held}, window seen {seen}, moved {moved}, arrived {arrived}, gathered {gathered}, distributed {distributed}, grouped {grouped}"),
    );
}
