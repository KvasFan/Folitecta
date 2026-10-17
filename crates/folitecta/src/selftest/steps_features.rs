//! Interaction steps 35–40: tables, accessibility, the accounting calculator, metadata and files, the JavaScript console and media.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir, z) = (cx.st, cx.dir, cx.z);
    // 35. Tables: one drawn with the table tool, a cell typed, two cells
    // merged and a row added in the editor; then copies of a form field and
    // the tab order numbers of the fields.
    let count = |st: AppState| {
        st.read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
            .unwrap_or(0)
    };
    let before = count(st);
    crate::commands::dispatch(st, "tool.table.create");
    pause(300).await;
    let _ = pointer_path(0, z, &line((330.0, 430.0), (560.0, 530.0), 6), false).await;
    pause(500).await;
    let asked = matches!((st.dialog)(), Dialog::TableNew { .. });
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let editor = (st.dialog)() == Dialog::TableEditor;
    let _ = document::eval(
        "const a = document.querySelector('.table-grid textarea'); if (a) { a.value = 'Item'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval(
        "const c = document.querySelectorAll('.table-grid .table-cell'); c[3]?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true })); c[4]?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, shiftKey: true }));",
    )
    .await;
    pause(300).await;
    crate::commands::dispatch(st, "tool.table.cell.merge");
    pause(300).await;
    crate::commands::dispatch(st, "tool.table.insert.rows.below");
    pause(400).await;
    let _ = capture_window(&dir.join("interaction-table-editor.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let after = count(st);
    let table = st
        .read_active(|t| t.doc.table_at(0, after.saturating_sub(1)))
        .flatten();
    let _ = capture_window(&dir.join("interaction-table.png"));
    cx.record(
        "table drawn and edited",
        asked
            && editor
            && after == before + 1
            && table.as_ref().is_some_and(|t| {
                t.cells[0][0].text == "Item" && t.cells[1][0].colspan == 2 && t.row_count() == 4
            }),
        format!(
            "{before} -> {after} annotations, {:?}",
            table.as_ref().map(|t| (t.row_count(), t.col_count()))
        ),
    );
    st.with_active(|t| t.selected_annot = None);
    crate::commands::dispatch(st, "tool.editFields");
    pause(300).await;
    let widget = st
        .with_active(|t| {
            let f = t
                .doc
                .form_fields()
                .into_iter()
                .find(|f| f.name == "contact.name")?;
            let wid = *f.widgets.first()?;
            let i = t
                .doc
                .annotations(0)
                .ok()?
                .into_iter()
                .position(|a| a.id == Some(wid))?;
            t.selected_annot = Some((0, i));
            Some(f.id)
        })
        .flatten();
    let fields_before = st.read_active(|t| t.doc.form_fields().len()).unwrap_or(0);
    crate::commands::dispatch(st, "tool.editFields.createMulti");
    pause(400).await;
    let copies_asked = matches!((st.dialog)(), Dialog::FieldCopies(f) if Some(f) == widget);
    let _ = document::eval(
        "const i = document.querySelectorAll('.dialog input'); const set = (e, v) => { if (e) { e.value = v; e.dispatchEvent(new Event('input', { bubbles: true })); } }; set(i[0], '1'); set(i[1], '2');",
    )
    .await;
    pause(250).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let fields_after = st.read_active(|t| t.doc.form_fields().len()).unwrap_or(0);
    cx.record(
        "copies of a form field created",
        copies_asked && fields_after == fields_before + 1,
        format!("{fields_before} -> {fields_after} fields"),
    );
    crate::commands::dispatch(st, "tool.editFields.showTabNumbers");
    pause(500).await;
    let numbers = document::eval("return document.querySelectorAll('#pg-0 .field-mark .num').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&dir.join("interaction-field-tab-numbers.png"));
    cx.record(
        "tab order numbers shown on the fields",
        numbers as usize >= 4,
        format!("{numbers} numbers"),
    );
    crate::commands::dispatch(st, "tool.editFields.showTabNumbers");
    st.set_tool(Tool::Hand);

    // 36. Accessibility: automatic tags, the tags, order and content panes,
    // the check with a fix, and the report.
    let count_js = |sel: &str| format!("return document.querySelectorAll('{sel}').length;");
    let js_count = |sel: String| async move {
        document::eval(&sel)
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    };
    st.navigate(0, true);
    crate::commands::dispatch(st, "acc.autotag.document");
    pause(900).await;
    let tagged = st
        .read_active(|t| t.doc.has_structure() && t.doc.is_tagged())
        .unwrap_or(false);
    let tag_rows = js_count(count_js(".tag-tree .tag-row")).await;
    let _ = capture_window(&dir.join("interaction-tags.png"));
    cx.record(
        "document tagged automatically",
        tagged && tag_rows > 3,
        format!("{tag_rows} tag rows"),
    );
    let _ = document::eval("document.querySelectorAll('.tag-tree .tag-row')[2]?.click();").await;
    pause(300).await;
    let picked = crate::commands::accessibility::selected_tag(st);
    let elems = |st: AppState| {
        st.read_active(|t| {
            let mut n = 0;
            if let Some(tree) = t.doc.structure() {
                tree.walk(&mut |_, _| n += 1);
            }
            n
        })
        .unwrap_or(0)
    };
    let before = elems(st);
    crate::commands::dispatch(st, "tagsview.new");
    pause(300).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    cx.record(
        "tag selected and a new tag added",
        picked.is_some() && elems(st) == before + 1,
        format!("{before} -> {} elements", elems(st)),
    );
    crate::commands::show_pane(st, Pane::Order);
    pause(600).await;
    let order_rows = js_count(count_js(".order-list .order-row")).await;
    let boxes = js_count(count_js("#pg-0 .order-box")).await;
    let _ = document::eval("document.querySelector('.order-list .order-row')?.click();").await;
    pause(300).await;
    let _ = capture_window(&dir.join("interaction-order.png"));
    cx.record(
        "reading order listed and shown on the page",
        order_rows > 0 && boxes as usize >= order_rows as usize,
        format!("{order_rows} items, {boxes} boxes"),
    );
    crate::commands::show_pane(st, Pane::Content);
    pause(500).await;
    let content_rows = js_count(count_js(".content-list .content-row")).await;
    let _ = capture_window(&dir.join("interaction-content.png"));
    cx.record(
        "content pane lists the marked content",
        content_rows > 0,
        format!("{content_rows} rows"),
    );
    crate::commands::dispatch(st, "acc.check.document");
    pause(900).await;
    let rules = js_count(count_js(".acc-tree .acc-rule")).await;
    let lang_failed = st
        .read_active(|t| {
            t.acc.check.as_ref().is_some_and(|c| {
                c.results.iter().any(|r| {
                    r.rule == "language" && r.status == folitecta_core::accessibility::CheckStatus::Failed
                })
            })
        })
        .unwrap_or(false);
    let _ = capture_window(&dir.join("interaction-checker.png"));
    st.with_active(|t| {
        if let Some(c) = t.acc.check.as_mut() {
            c.rule = Some("language".into());
        }
    });
    crate::commands::dispatch(st, "chekerView.fix");
    pause(400).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let lang_passed = st
        .read_active(|t| {
            t.acc.check.as_ref().is_some_and(|c| {
                c.results.iter().any(|r| {
                    r.rule == "language" && r.status == folitecta_core::accessibility::CheckStatus::Passed
                })
            })
        })
        .unwrap_or(false);
    cx.record(
        "accessibility check with a fix",
        rules as usize == folitecta_core::accessibility::RULES.len() && lang_failed && lang_passed,
        format!("{rules} rules"),
    );
    crate::commands::dispatch(st, "chekerView.showReport");
    pause(500).await;
    let report_rows = js_count(count_js(".acc-report .acc-report-row")).await;
    let _ = capture_window(&dir.join("interaction-report.png"));
    cx.record(
        "accessibility report shown",
        report_rows > 0,
        format!("{report_rows} rows"),
    );
    st.ui_mut().left_pane = "thumbnails".into();
    pause(300).await;

    // 37. Accounting calculator: a tape placed on the page, amounts typed with
    // the keypad, decimals changed and taken back in the pane.
    crate::commands::dispatch(st, "comments.accountingCalc");
    pause(300).await;
    let _ = pointer_path(0, z, &[(380.0, 120.0), (380.0, 120.0)], false).await;
    pause(600).await;
    let opened = crate::commands::calculator::CALC_EDIT.peek().is_some();
    for (amount, op) in [("125.5", "+"), ("20", "−"), ("", "=")] {
        let js = format!(
            "const i = document.getElementById('calc-entry'); if (i) {{ i.value = '{amount}'; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}"
        );
        let _ = document::eval(&js).await;
        pause(150).await;
        let js = format!(
            "[...document.querySelectorAll('.calc-key.op')].find(b => b.textContent.trim() === '{op}')?.click();"
        );
        let _ = document::eval(&js).await;
        pause(350).await;
    }
    let tape = |st: AppState| {
        crate::commands::calculator::CALC_EDIT
            .peek()
            .as_ref()
            .and_then(|e| st.read_active(|t| t.doc.calculator_at(e.page, e.index)).flatten())
    };
    let typed = tape(st);
    crate::commands::dispatch(st, "ac.decimal.zero");
    pause(300).await;
    let zero = tape(st).map(|t| t.decimals);
    let ctx_tab = document::eval("return [...document.querySelectorAll('.ribbon-tab.ctx')].length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&dir.join("interaction-calculator.png"));
    crate::commands::dispatch(st, "ac.undo");
    pause(300).await;
    let back = tape(st).map(|t| t.decimals);
    cx.record(
        "calculator tape typed on the keypad",
        opened
            && typed
                .as_ref()
                .is_some_and(|t| t.lines.len() == 3 && (t.total() - 105.5).abs() < 1e-9)
            && zero == Some(Some(0))
            && back == Some(Some(2))
            && (!st.ui.peek().ribbon || ctx_tab > 0),
        format!("{:?}", typed.map(|t| t.text())),
    );
    crate::commands::dispatch(st, "comments.ac.closeAll");
    pause(300).await;
    st.with_active(|t| t.selected_annot = None);

    // 38. Metadata templates, guides copied to every page, the continuous
    // zoom tool and renaming the document file.
    crate::commands::dispatch(st, "xmp.export.template");
    pause(300).await;
    let _ = document::eval(
        "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Team'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let saved = crate::commands::fileops::templates(st)
        .iter()
        .any(|x| x.name == "Team");
    let listed = (st.dialog)() == Dialog::MetaTemplates;
    let _ = capture_window(&dir.join("interaction-metadata.png"));
    st.set_dialog(Dialog::None);
    cx.record(
        "metadata template saved and listed",
        saved && listed,
        String::new(),
    );
    st.with_active(|t| {
        t.guides.push(crate::state::Guide {
            page: Some(0),
            horizontal: true,
            pos: 123.0,
        })
    });
    st.navigate(0, true);
    let copied = crate::commands::fileops::duplicate_guides(st);
    let imported =
        crate::commands::fileops::import_guides_from(st, r#"[{"page":null,"horizontal":false,"pos":77.0}]"#);
    let pages = st.page_count();
    let n123 = st
        .read_active(|t| t.guides.iter().filter(|g| g.pos == 123.0).count())
        .unwrap_or(0);
    cx.record(
        "guides copied to every page and imported",
        copied + 1 == pages && n123 == pages && imported == Ok(1),
        format!("{copied} copies, {n123} on {pages} pages"),
    );
    st.with_active(|t| t.guides.clear());
    crate::commands::dispatch(st, "tool.zoomDyn");
    pause(200).await;
    let z0 = crate::commands::current_zoom(st);
    let _ = pointer_path(0, z0, &[(200.0, 400.0), (200.0, 370.0), (200.0, 340.0)], false).await;
    pause(400).await;
    let z1 = crate::commands::current_zoom(st);
    cx.record(
        "continuous zoom by dragging up",
        z1 > z0 * 1.3,
        format!("{z0:.2} -> {z1:.2}"),
    );
    st.set_tool(Tool::Hand);
    st.with_active(|t| t.zoom = crate::state::Zoom::FitWidth);
    let file = dir.join("rename-me.pdf");
    let renamed = dir.join("renamed-doc.pdf");
    let _ = std::fs::remove_file(&renamed);
    let written = st
        .read_active(|t| std::fs::write(&file, t.doc.bytes().as_slice()).is_ok())
        .unwrap_or(false);
    crate::files::open_path(st, file.clone(), None);
    pause(600).await;
    crate::commands::dispatch(st, "renameDoc");
    pause(300).await;
    let _ = document::eval(
        "const i = document.querySelector('.dialog input'); if (i) { i.value = 'renamed-doc'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let now = st
        .read_active(|t| t.doc.path().map(|p| p.to_path_buf()))
        .flatten();
    cx.record(
        "document file renamed",
        written && renamed.exists() && !file.exists() && now.as_deref() == Some(renamed.as_path()),
        format!("{now:?}"),
    );
    if let Some(id) = st
        .read_active(|t| (t.doc.path() == Some(renamed.as_path())).then_some(t.id))
        .flatten()
    {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    st.set_active(0);
    pause(300).await;

    // 39. JavaScript console: a script that prints, returns and changes a
    // field and the title; an endless loop stopped by the time limit; a
    // snippet; a named document script.
    let wait_console = || async {
        for _ in 0..60 {
            pause(100).await;
            if !crate::commands::javascript::CONSOLE.peek().running {
                break;
            }
        }
    };
    crate::commands::dispatch(st, "jsConsole.show");
    pause(300).await;
    crate::commands::javascript::CONSOLE.write().output.clear();
    st.set_value(
        "js.code",
        "// Fill the name\nconsole.println(this.numPages);\nthis.getField('contact.name').value = 'Ada';\nthis.title = 'Scripted';\nthis.numPages * 2",
    );
    crate::commands::dispatch(st, "jsConsole.run");
    wait_console().await;
    pause(300).await;
    let pages = st.page_count();
    let output = crate::commands::javascript::CONSOLE.peek().output.clone();
    let printed = output.iter().any(|(k, v)| k == "out" && *v == pages.to_string());
    let returned = output
        .iter()
        .any(|(k, v)| k == "result" && *v == (pages * 2).to_string());
    let field = st
        .read_active(|t| {
            t.doc
                .form_fields()
                .into_iter()
                .find(|f| f.name == "contact.name")
                .map(|f| f.value)
        })
        .flatten()
        .unwrap_or_default();
    let title = st.read_active(|t| t.doc.title()).unwrap_or_default();
    let _ = capture_window(&dir.join("interaction-js-console.png"));
    cx.record(
        "JavaScript console runs a script that changes the document",
        printed && returned && field == "Ada" && title == "Scripted",
        format!("{output:?}"),
    );
    st.set_value("js.timeout", "1");
    st.set_value("js.code", "while (true) {}");
    crate::commands::dispatch(st, "jsConsole.run");
    wait_console().await;
    let stopped = crate::commands::javascript::CONSOLE
        .peek()
        .output
        .last()
        .is_some_and(|(k, _)| k == "error");
    st.set_value("js.timeout", "");
    st.set_value("js.code", "// Fill the name\nthis.getField('contact.name').value");
    crate::commands::dispatch(st, "jssnippet.add");
    pause(300).await;
    let snippet = crate::commands::javascript::snippets()
        .iter()
        .any(|(n, _)| n == "Fill the name");
    cx.record(
        "endless script stopped, snippet saved",
        stopped && snippet,
        String::new(),
    );
    crate::commands::dispatch(st, "javaScriptList.add");
    pause(400).await;
    let _ = document::eval(
        "const i = document.querySelector('.js-script-edit input'); if (i) { i.value = 'Totals'; i.dispatchEvent(new Event('input', { bubbles: true })); } const a = document.querySelector('.js-script-edit textarea'); if (a) { a.value = 'var total = 0;'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let _ = capture_window(&dir.join("interaction-js-scripts.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(400).await;
    let scripts = st.read_active(|t| t.doc.document_scripts()).unwrap_or_default();
    cx.record(
        "named document script added",
        scripts
            .iter()
            .any(|(n, c)| n == "Totals" && c == "var total = 0;"),
        format!("{scripts:?}"),
    );
    st.set_dialog(Dialog::None);

    // 40. Sound and video comments: a sound and an embedded audio file, the
    // player loading the media, the media list and the properties.
    let tone: Vec<u8> = {
        let pcm: Vec<u8> = (0..16000i32)
            .flat_map(|i| (((i as f32 * 0.12).sin() * 6000.0) as i16).to_le_bytes())
            .collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&16000u32.to_le_bytes());
        w.extend_from_slice(&32000u32.to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(&pcm);
        w
    };
    let before = st.read_active(|t| t.doc.media_items().len()).unwrap_or(0);
    let added = st
        .with_active(|t| {
            let a = t.doc.add_sound(0, (40.0, 300.0), &tone, "Reviewer").is_ok();
            let b = t
                .doc
                .add_media(
                    0,
                    [70.0, 200.0, 230.0, 300.0],
                    "tone.wav",
                    tone.clone(),
                    "Reviewer",
                )
                .is_ok();
            t.refresh();
            a && b
        })
        .unwrap_or(false);
    let items = st.read_active(|t| t.doc.media_items()).unwrap_or_default();
    let last = items.last().map(|m| (m.page, m.index));
    let mut loaded = 0.0;
    if let Some((p, i)) = last {
        crate::commands::media::play_item(st, p, i);
        for _ in 0..40 {
            pause(150).await;
            loaded = document::eval(
                "const m = document.getElementById('folitecta-media'); return m ? (m.duration || 0) : 0;",
            )
            .await
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
            if loaded > 0.0 {
                break;
            }
        }
    }
    let _ = capture_window(&dir.join("interaction-media-player.png"));
    crate::commands::dispatch(st, "media.stop");
    cx.record(
        "sound and audio comments, media loaded in the player",
        added && items.len() == before + 2 && (loaded - 1.0).abs() < 0.1,
        format!("{} items, duration {loaded:.2}", items.len()),
    );
    crate::commands::media::close_player();
    crate::commands::dispatch(st, "media.list");
    pause(400).await;
    let rows = document::eval("return document.querySelectorAll('.media-list .media-row').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&dir.join("interaction-media-list.png"));
    if let Some((p, i)) = last {
        st.set_dialog(Dialog::MediaProps { page: p, index: i });
        pause(300).await;
        let _ = document::eval(
            "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Tone'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
        )
        .await;
        pause(200).await;
        let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
        pause(400).await;
    }
    let titled = st
        .read_active(|t| t.doc.media_items().last().map(|m| m.title.clone()))
        .flatten()
        .unwrap_or_default();
    cx.record(
        "media list and properties",
        rows as usize == before + 2 && titled == "Tone",
        format!("{rows} rows, title {titled}"),
    );
    st.set_dialog(Dialog::None);
}
