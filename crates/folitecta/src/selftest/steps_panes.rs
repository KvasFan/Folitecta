//! Interaction steps 25–34: macros, layers, attachments, fields, search, portfolios, history and sessions, stamps, document tools and comparison.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir, z) = (cx.st, cx.dir, cx.z);
    // 25. Macros: insert them into a header field with the picker and the menu
    // command, then check the expansion on the page.
    st.set_dialog(Dialog::HeaderFooter);
    pause(600).await;
    let _ = document::eval(
        "const f = document.querySelector('.dialog input:not([type])'); if (f) { f.focus(); f.setSelectionRange(f.value.length, f.value.length); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval(
        "const s = document.querySelector('.macro-picker select'); if (s) { s.value = '{label}'; s.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(400).await;
    crate::commands::dispatch(st, "macro.page");
    pause(500).await;
    let field = document::eval("return document.querySelector('.dialog input:not([type])')?.value ?? '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    cx.record(
        "macros inserted into the focused field",
        field == "{title}{label}{page}",
        field.clone(),
    );
    let _ = capture_window(&dir.join("interaction-macros.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let (expected, text) = st
        .read_active(|t| (t.doc.expand_macros(&field, t.page, 1), t.doc.page_text(t.page)))
        .unwrap_or_default();
    cx.record(
        "macros expanded in the header",
        !expected.contains('{') && text.contains(&expected),
        expected,
    );

    // 26. Layers: a new layer from the pane, pages imported as a layer, view
    // visibility made the default, then everything flattened.
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "layers".into();
        ui.show_left_pane = true;
    }
    crate::commands::dispatch(st, "layersView.addNewLayer");
    pause(500).await;
    let _ = document::eval(
        "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Review marks'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(250).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let names = st
        .read_active(|t| t.doc.layers().iter().map(|l| l.name.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    cx.record(
        "layer created from the layers pane",
        names == ["Review marks"],
        names.join(", "),
    );
    let source = dir.join("layer-source.pdf");
    let written = st
        .read_active(|t| std::fs::write(&source, t.doc.bytes().as_slice()).is_ok())
        .unwrap_or(false);
    st.set_dialog(Dialog::ImportLayer(source.clone()));
    pause(500).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
    let count = st.read_active(|t| t.doc.layers().len()).unwrap_or(0);
    cx.record(
        "pages imported as a layer",
        written && count == 2,
        format!("{count} layers"),
    );
    let _ = document::eval(
        "const b = [...document.querySelectorAll('.layer-row .ibtn.eye')].pop(); if (b) b.click();",
    )
    .await;
    pause(600).await;
    let (overrides, defaults_hidden) = st
        .read_active(|t| {
            (
                t.layer_view.len(),
                t.doc.layers().iter().filter(|l| !l.visible).count(),
            )
        })
        .unwrap_or((0, 9));
    let foot = document::eval("return !!document.querySelector('.layer-foot');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    cx.record(
        "layer hidden in the view only",
        overrides == 1 && defaults_hidden == 0 && foot,
        format!("{overrides} override(s), {defaults_hidden} hidden by default"),
    );
    let _ = capture_window(&dir.join("interaction-layers.png"));
    crate::commands::dispatch(st, "layersView.setCurrentStatesAsDefault");
    pause(500).await;
    let (overrides, defaults_hidden) = st
        .read_active(|t| {
            (
                t.layer_view.len(),
                t.doc.layers().iter().filter(|l| !l.visible).count(),
            )
        })
        .unwrap_or((9, 0));
    cx.record(
        "view visibility made the default",
        overrides == 0 && defaults_hidden == 1,
        format!("{overrides} override(s), {defaults_hidden} hidden by default"),
    );
    crate::commands::dispatch(st, "layersView.flattenOC");
    pause(600).await;
    let left = st.read_active(|t| t.doc.layers().len()).unwrap_or(9);
    cx.record("layers flattened", left == 0, format!("{left} left"));
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    st.ui_mut().left_pane = "thumbnails".into();
    let _ = std::fs::remove_file(&source);

    // 27. Attachments pane: select an attachment and edit its description.
    st.with_active(|t| {
        let _ = t.doc.add_attachment(
            "notes.txt",
            b"Notes for the review".to_vec(),
            "",
            Some("text/plain"),
        );
        t.refresh();
    });
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "attachments".into();
        ui.show_left_pane = true;
    }
    pause(500).await;
    let _ = document::eval("document.querySelector('.list-item.attachment')?.click();").await;
    pause(300).await;
    crate::commands::dispatch(st, "attachmentsView.edit");
    pause(500).await;
    let _ = document::eval(
        "const a = document.querySelector('.dialog textarea'); if (a) { a.value = 'Reviewer notes'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(250).await;
    let _ = capture_window(&dir.join("interaction-attachment-edit.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let desc = st
        .read_active(|t| t.doc.attachments().first().map(|a| a.description.clone()))
        .flatten()
        .unwrap_or_default();
    cx.record("attachment selected and edited", desc == "Reviewer notes", desc);

    // 28. Fields pane: name groups, selection by type, manual tab order.
    let added = st
        .with_active(|t| {
            let mut ids = Vec::new();
            for (i, name) in ["contact.name", "contact.email", "contact.phone"]
                .iter()
                .enumerate()
            {
                let y = 700.0 - i as f32 * 40.0;
                let props = folitecta_core::FieldProps {
                    name: name.to_string(),
                    ..folitecta_core::FieldProps::default()
                };
                ids.push(
                    t.doc
                        .add_form_field(
                            0,
                            folitecta_core::NewField::Text,
                            [60.0, y - 24.0, 300.0, y],
                            &props,
                        )
                        .ok()?,
                );
            }
            t.refresh();
            Some(ids.len())
        })
        .flatten()
        .unwrap_or(0);
    st.ui_mut().left_pane = "fields".into();
    pause(500).await;
    let groups = document::eval("return document.querySelectorAll('.field-tree .field-group').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    crate::commands::dispatch(st, "fieldsView.select.text");
    pause(300).await;
    let selected = st.read_active(|t| t.selected_fields.len()).unwrap_or(0);
    cx.record(
        "fields grouped by name and selected by type",
        added == 3 && groups >= 1 && selected >= 3,
        format!("{groups} group(s), {selected} selected"),
    );
    crate::commands::dispatch(st, "fieldsView.tabOrder.manually");
    pause(500).await;
    let before = st.read_active(|t| t.doc.widget_order(0)).unwrap_or_default();
    let _ = document::eval(
        "const b = document.querySelectorAll('.field-tree .field-row .ibtn')[1]; if (b) b.click();",
    )
    .await;
    pause(500).await;
    let after = st.read_active(|t| t.doc.widget_order(0)).unwrap_or_default();
    let _ = capture_window(&dir.join("interaction-fields-tab-order.png"));
    cx.record(
        "field moved in the manual tab order",
        before.len() >= 3 && after.len() == before.len() && after[0] == before[1] && after[1] == before[0],
        format!("{} widgets", after.len()),
    );
    st.set_value("fields.manual", "");
    st.ui_mut().left_pane = "thumbnails".into();

    // 29. Search: comments as well as the page text, words on the same page,
    // grouped results that collapse.
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "search".into();
        ui.show_left_pane = true;
    }
    pause(400).await;
    crate::commands::dispatch(st, "searchView.includeAnnotations");
    crate::ui::panes::run_search(st, "review");
    pause(400).await;
    let comments_found = st
        .read_active(|t| {
            t.search
                .other
                .iter()
                .any(|o| matches!(o.found_in, folitecta_core::search::FoundIn::Comment { .. }))
        })
        .unwrap_or(false);
    cx.record("search finds comments", comments_found, String::new());
    st.set_value("search.proximity", "page");
    crate::ui::panes::run_search(st, "lazy fox");
    pause(500).await;
    let words: std::collections::BTreeSet<usize> = st
        .read_active(|t| t.search.hits.iter().map(|h| h.word).collect())
        .unwrap_or_default();
    cx.record(
        "words found on the same page",
        words.len() == 2,
        format!("{words:?}"),
    );
    let _ = document::eval("[...document.querySelectorAll('.search-opts .linkish')].pop()?.click();").await;
    pause(400).await;
    let _ = capture_window(&dir.join("interaction-search-options.png"));
    crate::commands::dispatch(st, "searchView.results.collapseAll");
    pause(300).await;
    let items = document::eval("return document.querySelectorAll('.search-results .list-item').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(99);
    cx.record("result groups collapse", items == 0, format!("{items} visible"));
    crate::commands::dispatch(st, "searchView.results.expandAll");
    crate::commands::dispatch(st, "searchView.includeAnnotations");
    st.set_value("search.proximity", "");
    crate::ui::panes::run_search(st, "");
    st.ui_mut().left_pane = "thumbnails".into();

    // 30. Portfolio: files view, a new folder, navigation, preview, properties.
    let sample_bytes = st.read_active(|t| t.doc.bytes().to_vec()).unwrap_or_default();
    match folitecta_core::Document::new_portfolio(
        vec![
            ("report.pdf".into(), sample_bytes),
            ("notes.txt".into(), b"Portfolio notes".to_vec()),
        ],
        "Project portfolio",
    ) {
        Ok(doc) => {
            crate::files::open_document_tab(st, doc);
            pause(800).await;
            let shown =
                document::eval("return document.querySelectorAll('.portfolio .pf-row:not(.head)').length;")
                    .await
                    .ok()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            cx.record("portfolio shows its files", shown == 2, format!("{shown} rows"));
            crate::commands::dispatch(st, "view.portfolio.createFolder");
            pause(400).await;
            let _ = document::eval(
                "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Specs'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
            )
            .await;
            pause(200).await;
            let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
            pause(500).await;
            let _ = document::eval(
                "const r = [...document.querySelectorAll('.portfolio .pf-row')].find(e => e.textContent.includes('Specs')); if (r) r.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));",
            )
            .await;
            pause(400).await;
            let inside = st.read_active(|t| t.portfolio.folder.is_some()).unwrap_or(false);
            crate::commands::dispatch(st, "view.portfolio.view.nav.up");
            pause(300).await;
            let back = st.read_active(|t| t.portfolio.folder.is_none()).unwrap_or(false);
            cx.record(
                "portfolio folder created and opened",
                inside && back,
                String::new(),
            );
            let _ = document::eval(
                "const r = [...document.querySelectorAll('.portfolio .pf-row')].find(e => e.textContent.includes('report.pdf')); if (r) r.click();",
            )
            .await;
            let mut thumb = false;
            for _ in 0..30 {
                pause(150).await;
                thumb = document::eval("return !!document.querySelector('.portfolio-preview img.pf-thumb');")
                    .await
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if thumb {
                    break;
                }
            }
            cx.record("portfolio file previewed", thumb, String::new());
            let _ = capture_window(&dir.join("interaction-portfolio.png"));
            st.set_dialog(Dialog::PortfolioProps);
            pause(500).await;
            let _ = capture_window(&dir.join("interaction-portfolio-props.png"));
            st.set_dialog(Dialog::None);
            if let Some(i) = st.active_index() {
                st.docs_mut().remove(i);
                st.set_active(0);
            }
            pause(400).await;
        }
        Err(e) => cx.record("portfolio shows its files", false, e.to_string()),
    }

    // 31. History and sessions: open and close a saved file, pin it, save the
    // open documents as a session and open it again.
    let file = dir.join("history-sample.pdf");
    let written = st
        .read_active(|t| std::fs::write(&file, t.doc.bytes().as_slice()).is_ok())
        .unwrap_or(false);
    crate::files::open_path(st, file.clone(), None);
    pause(600).await;
    let tab = st
        .read_active(|t| (t.doc.path() == Some(file.as_path())).then_some(t.id))
        .flatten();
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    pause(400).await;
    let entry = st.ui.read().history.iter().find(|h| h.path == file).cloned();
    let thumb = crate::commands::history::thumb_path(&file).is_some_and(|p| p.exists());
    cx.record(
        "history keeps opened files with a thumbnail",
        written && entry.as_ref().is_some_and(|e| e.closed > 0 && e.pages > 0) && thumb,
        format!("{entry:?}"),
    );
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "history".into();
        ui.show_left_pane = true;
    }
    pause(400).await;
    let _ = document::eval("[...document.querySelectorAll('.history-item')].find(e => e.textContent.includes('history-sample'))?.click();").await;
    pause(200).await;
    crate::commands::dispatch(st, "historyList.pin");
    pause(300).await;
    let pinned = st.ui.read().history.iter().any(|h| h.path == file && h.pinned);
    cx.record("history entry pinned", pinned, String::new());
    let _ = capture_window(&dir.join("interaction-history.png"));
    crate::files::open_path(st, file.clone(), None);
    pause(500).await;
    crate::commands::dispatch(st, "saveCurrentSession");
    pause(400).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let saved = st
        .ui
        .read()
        .sessions
        .iter()
        .any(|s| s.files.iter().any(|f| f.path == file));
    if let Some(id) = st
        .read_active(|t| (t.doc.path() == Some(file.as_path())).then_some(t.id))
        .flatten()
    {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    st.ui_mut().left_pane = "sessions".into();
    pause(400).await;
    let n = st.ui.read().sessions.len();
    *st.session_sel.write_unchecked() = Some((n.saturating_sub(1), None));
    crate::commands::dispatch(st, "sessions.open.addWholeSession");
    pause(600).await;
    let reopened = st
        .docs
        .read()
        .iter()
        .any(|t| t.doc.path() == Some(file.as_path()));
    let _ = capture_window(&dir.join("interaction-sessions.png"));
    cx.record(
        "session saved and opened again",
        saved && reopened,
        format!("{n} session(s)"),
    );
    if let Some(id) = st
        .read_active(|t| (t.doc.path() == Some(file.as_path())).then_some(t.id))
        .flatten()
    {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    st.set_active(0);
    st.ui_mut().left_pane = "thumbnails".into();
    pause(400).await;

    // 32. Stamps: a text stamp in the library, a designed stamp from a
    // template with a counter, placed on the page.
    {
        let mut ui = st.ui_mut();
        ui.left_pane = "stamps".into();
        ui.show_left_pane = true;
    }
    crate::commands::dispatch(st, "stampsView.addCustom");
    pause(400).await;
    let _ = document::eval(
        "const i = [...document.querySelectorAll('.dialog input')].find(e => e.placeholder === 'PAID'); if (i) { i.value = 'PAID'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(250).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let has_text = crate::commands::stamps::library()
        .collections
        .iter()
        .any(|c| c.stamps.iter().any(|s| s.text == "PAID"));
    cx.record("text stamp added to the library", has_text, String::new());
    crate::commands::dispatch(st, "stampsView.addNew");
    pause(300).await;
    crate::commands::dispatch(st, "stampCustom.openFromTemplate");
    pause(300).await;
    let _ = document::eval("document.querySelector('.stamp-template')?.click();").await;
    pause(300).await;
    crate::commands::dispatch(st, "stampCustom.add.field.counter");
    pause(400).await;
    let elements = crate::commands::stamps::DESIGNER.peek().design.elements.len();
    let _ = capture_window(&dir.join("interaction-stamp-designer.png"));
    let before = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    let counter = crate::commands::stamps::library().counter;
    crate::commands::dispatch(st, "stampCustom.addStamp");
    pause(400).await;
    let _ = pointer_path(0, z, &[(200.0, 300.0), (200.0, 300.0)], false).await;
    pause(600).await;
    let after = st
        .read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    let counted = crate::commands::stamps::library().counter == counter + 1;
    cx.record(
        "designed stamp placed with its counter",
        elements >= 2 && after == before + 1 && counted,
        format!("{elements} elements, {before} -> {after} annotations"),
    );
    let _ = capture_window(&dir.join("interaction-stamps.png"));
    st.set_tool(Tool::Hand);
    st.ui_mut().left_pane = "thumbnails".into();

    // 33. Document tools: typed text inserted as pages, space audit,
    // document actions.
    let pages_before = st.page_count();
    crate::commands::dispatch(st, "document.insertText");
    pause(400).await;
    let _ = document::eval(
        "const a = document.querySelector('.dialog textarea'); if (a) { a.value = 'Inserted page\\nwith two lines'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let pages_after = st.page_count();
    cx.record(
        "typed text inserted as a page",
        pages_after == pages_before + 1,
        format!("{pages_before} -> {pages_after}"),
    );
    crate::commands::dispatch(st, "document.audit");
    pause(500).await;
    let rows = document::eval("return document.querySelectorAll('.audit-row:not(.head)').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&dir.join("interaction-audit.png"));
    cx.record("space audit lists the content", rows >= 4, format!("{rows} rows"));
    st.set_dialog(Dialog::None);
    crate::commands::dispatch(st, "document.actions");
    pause(400).await;
    let _ = document::eval(
        "const a = document.querySelector('.dialog textarea'); if (a) { a.value = 'console.println(\"opened\");'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let actions = st.read_active(|t| t.doc.document_actions()).unwrap_or_default();
    cx.record(
        "document action saved",
        actions.iter().any(|(e, c)| e == "Open" && c.contains("opened")),
        format!("{actions:?}"),
    );
    st.set_dialog(Dialog::None);

    // 34. Compare two documents: differences listed, highlighted, visited.
    let make = |text: &str| folitecta_core::Document::from_text(text, Default::default());
    if let (Ok(a), Ok(b)) = (
        make("Folitecta compares documents word by word.\n\nThe old version has a brown fox and a lazy dog."),
        make(
            "Folitecta compares documents word by word.\n\nThe new version has a red fox and a dog that sleeps.",
        ),
    ) {
        crate::files::open_document_tab(st, a);
        let old = st.read_active(|t| t.id).unwrap_or(0);
        crate::files::open_document_tab(st, b);
        let new = st.read_active(|t| t.id).unwrap_or(0);
        crate::commands::compare::run(st, old, new, false);
        let mut found = 0;
        for _ in 0..40 {
            pause(150).await;
            found = st
                .comparison
                .read()
                .as_ref()
                .map(|c| c.result.differences.len())
                .unwrap_or(0);
            if found > 0 {
                break;
            }
        }
        pause(500).await;
        let marks = document::eval("return document.querySelectorAll('#canvas .cmp').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        crate::commands::dispatch(st, "comparisonView.next");
        pause(400).await;
        let current = st.comparison.read().as_ref().map(|c| c.current).unwrap_or(0);
        let _ = capture_window(&dir.join("interaction-compare.png"));
        cx.record(
            "documents compared with highlighted differences",
            found >= 3 && marks > 0 && current == 1,
            format!("{found} differences, {marks} marks"),
        );
        *st.comparison.write_unchecked() = None;
        st.docs_mut().retain(|t| t.id != old && t.id != new);
        st.set_active(0);
        st.ui_mut().left_pane = "thumbnails".into();
        pause(400).await;
    }
}
