//! Interaction steps 20–24: comment review, bookmarks, automatic scrolling, digital signatures and handwritten signatures.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir, z) = (cx.st, cx.dir, cx.z);
    // 20. Comment review: reply from the comments pane, pop-up note, status,
    // filter and a summary document.
    st.set_tool(Tool::Hand);
    let first = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(0)
                .into_iter()
                .find(|c| c.reply_to.is_none() && !c.is_state)
        })
        .flatten();
    if let Some(c) = first {
        {
            let mut ui = st.ui_mut();
            ui.show_left_pane = true;
            ui.left_pane = "comments".into();
        }
        st.with_active(|t| t.selected_annot = Some((c.page, c.index)));
        pause(700).await;
        let _ = eval("const b = document.querySelector('.comment-item.selected .comment-actions .tbtn'); if (b) b.click();").await;
        pause(500).await;
        let _ = eval("const a = document.querySelector('.comment-item.selected .reply-box textarea'); if (a) { a.value = 'Agreed'; a.dispatchEvent(new Event('input', { bubbles: true })); } const s = document.querySelector('.comment-item.selected .reply-box .tbtn.primary'); if (s) s.click();").await;
        pause(700).await;
        let replies = st
            .read_active(|t| {
                t.doc
                    .comment_infos()
                    .iter()
                    .filter(|r| r.reply_to == Some(c.id) && r.contents == "Agreed")
                    .count()
            })
            .unwrap_or(0);
        cx.record(
            "reply written in the comments pane",
            replies == 1,
            format!("{replies} reply"),
        );
        st.with_active(|t| t.selected_annot = Some((c.page, c.index)));
        crate::commands::dispatch(st, "annot.popup.open");
        pause(900).await;
        let notes = eval("return document.querySelectorAll('.popup-note').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        cx.record(
            "pop-up note opens on the page",
            notes >= 1,
            format!("{notes} note(s)"),
        );
        crate::commands::review::set_state(st, c.page, c.index, "Review", "Accepted");
        pause(700).await;
        let review = st
            .read_active(|t| {
                t.doc
                    .comment_infos()
                    .into_iter()
                    .find(|x| x.id == c.id)
                    .map(|x| x.review)
            })
            .flatten()
            .unwrap_or_default();
        let chip = eval("return document.querySelectorAll('.comment-item .chip.state').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        cx.record(
            "review status shown in the pane",
            review == "Accepted" && chip >= 1,
            format!("{review}, {chip} chip(s)"),
        );
        let _ = capture_window(&dir.join("interaction-comment-review.png"));
        // Filter out the comment's author: the pane and the page view update.
        let author = c.author.clone();
        let gen_before = st.read_active(|t| t.view_gen).unwrap_or(0);
        crate::commands::review::update_filter(st, |f| {
            f.hidden_authors.insert(author.clone());
        });
        pause(700).await;
        let gen_after = st.read_active(|t| t.view_gen).unwrap_or(0);
        let listed = eval("return document.querySelectorAll('.comment-item').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(99);
        let expected = st
            .read_active(|t| {
                t.doc
                    .comment_infos()
                    .iter()
                    .filter(|x| x.reply_to.is_none() && !x.is_state && x.author != c.author)
                    .count()
            })
            .unwrap_or(0) as u64;
        cx.record(
            "comment filter hides an author",
            gen_after > gen_before && listed == expected,
            format!("{listed} listed, {expected} expected"),
        );
        crate::commands::review::update_filter(st, |f| *f = Default::default());
        // Summary document in a new tab.
        let tabs_before = st.docs.read().len();
        crate::commands::dispatch(st, "comments.summarize");
        pause(500).await;
        let _ =
            eval("const b = document.querySelector('.dialog .dialog-foot .tbtn.primary'); if (b) b.click();")
                .await;
        pause(1200).await;
        let tabs_after = st.docs.read().len();
        let summary_pages = if tabs_after > tabs_before {
            st.docs.read().last().map(|t| t.doc.page_count()).unwrap_or(0)
        } else {
            0
        };
        cx.record(
            "comment summary opens as a new document",
            tabs_after == tabs_before + 1 && summary_pages >= 1,
            format!("{tabs_before} -> {tabs_after} tabs, {summary_pages} page(s)"),
        );
        let _ = capture_window(&dir.join("interaction-comment-summary.png"));
        if tabs_after > tabs_before {
            st.docs_mut().pop();
            st.set_active(tabs_before - 1);
        }
        st.ui_mut().left_pane = "thumbnails".into();
        pause(500).await;
    } else {
        cx.record("reply written in the comments pane", false, "no comment".into());
    }

    // 21. Bookmarks: demote/promote, move by dragging, a new bookmark from the
    // current view, a named destination.
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "bookmarks".into();
    }
    let titles = |st: AppState| -> Vec<(usize, String)> {
        st.read_active(|t| {
            t.doc
                .outline_flat()
                .into_iter()
                .map(|(d, ti, _)| (d, ti))
                .collect()
        })
        .unwrap_or_default()
    };
    let before = titles(st);
    st.with_active(|t| t.selected_bookmark = Some(vec![1]));
    crate::commands::dispatch(st, "bookmarksView.demote");
    pause(500).await;
    let demoted = titles(st);
    cx.record(
        "demote a bookmark under the previous one",
        demoted.get(1).map(|(d, _)| *d) == Some(1)
            && st.read_active(|t| t.selected_bookmark.clone()).flatten() == Some(vec![0, 0]),
        format!("{:?}", demoted.iter().take(3).collect::<Vec<_>>()),
    );
    crate::commands::dispatch(st, "bookmarksView.promote");
    pause(500).await;
    cx.record("promote it back", titles(st) == before, String::new());
    // Drag the third bookmark onto the lower part of the first: it goes after it.
    pause(300).await;
    // The press, then the moves a moment later, as a hand would do them (the
    // rows follow the moves only once a drag has started).
    let _ = eval(
        "const items = [...document.querySelectorAll('.bookmark-tree .tree-item')]; const src = items[2]; if (src) { const s = src.getBoundingClientRect(); src.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: s.left + 30, clientY: s.top + 10 })); }",
    )
    .await;
    pause(300).await;
    let _ = eval(
        "const dst = document.querySelectorAll('.bookmark-tree .tree-item')[0]; if (dst) { const d = dst.getBoundingClientRect(); dst.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: d.left + 30, clientY: d.top + 20 })); }",
    )
    .await;
    pause(300).await;
    let _ = eval(
        "const dst = document.querySelectorAll('.bookmark-tree .tree-item')[0]; if (dst) { const d = dst.getBoundingClientRect(); dst.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: d.left + 30, clientY: d.top + 20 })); }",
    )
    .await;
    pause(600).await;
    let dragged = titles(st);
    cx.record(
        "drag a bookmark to a new place",
        dragged.get(1).map(|(_, t)| t.clone()) == before.get(2).map(|(_, t)| t.clone()),
        format!("{:?}", dragged.iter().take(3).collect::<Vec<_>>()),
    );
    let _ = capture_window(&dir.join("interaction-bookmarks.png"));
    st.with_active(|t| t.selected_bookmark = Some(vec![0]));
    let count_before = st.read_active(|t| t.doc.bookmark_count()).unwrap_or(0);
    crate::commands::dispatch(st, "bookmarksView.new");
    pause(900).await;
    let _ = eval("const i = document.querySelector('.dialog input'); if (i) { i.value = 'Self-test bookmark'; i.dispatchEvent(new Event('input', { bubbles: true })); } const b = document.querySelector('.dialog .dialog-foot .tbtn.primary'); if (b) b.click();").await;
    pause(600).await;
    let created = st
        .read_active(|t| {
            t.doc
                .outline_flat()
                .iter()
                .any(|(_, ti, _)| ti == "Self-test bookmark")
        })
        .unwrap_or(false);
    let count_after = st.read_active(|t| t.doc.bookmark_count()).unwrap_or(0);
    cx.record(
        "new bookmark from the current view, renamed",
        created && count_after == count_before + 1,
        format!("{count_before} -> {count_after}"),
    );
    crate::commands::dispatch(st, "pdfNamedDestsView.new");
    pause(900).await;
    let _ = eval("const b = document.querySelector('.dialog .dialog-foot .tbtn.primary'); if (b) b.click();")
        .await;
    pause(500).await;
    let dests = st.read_active(|t| t.doc.named_destinations().len()).unwrap_or(0);
    cx.record(
        "named destination from the current view",
        dests >= 1,
        format!("{dests} destination(s)"),
    );
    st.ui_mut().left_pane = "thumbnails".into();

    // 22. Automatic scrolling moves the pages; the pan and zoom window moves the view.
    st.with_active(|t| t.zoom = crate::state::Zoom::Fixed(1.5));
    st.go_to_page(1);
    pause(700).await;
    let scroll_top = || async {
        eval("const c = document.getElementById('canvas'); return c ? c.scrollTop : 0;")
            .await
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    let top0 = scroll_top().await;
    crate::commands::dispatch(st, "view.layout.autoScroll");
    pause(1000).await;
    let top1 = scroll_top().await;
    crate::commands::dispatch(st, "view.layout.autoScroll");
    pause(300).await;
    cx.record(
        "automatic scrolling moves the pages and stops",
        top1 > top0 + 5.0 && !crate::commands::interface::auto_scroll_on(),
        format!("{top0:.0} -> {top1:.0}"),
    );
    st.set_option("view.panzoom", true);
    pause(900).await;
    let before = scroll_top().await;
    let _ = eval("const p = document.querySelector('.panzoom-page'); if (p) { const r = p.getBoundingClientRect(); p.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height * 0.9 })); p.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height * 0.9 })); }").await;
    pause(600).await;
    let after = scroll_top().await;
    cx.record(
        "pan and zoom window moves the view",
        (after - before).abs() > 20.0,
        format!("{before:.0} -> {after:.0}"),
    );
    let _ = capture_window(&dir.join("interaction-panzoom.png"));
    st.set_option("view.panzoom", false);
    st.with_active(|t| t.zoom = crate::state::Zoom::FitWidth);

    // 23. Digital signature: a new ID, an invisible signature, validation in the
    // signatures pane and trusting the signer.
    let created = crate::commands::signatures::create_id(
        &folitecta_core::digsig::IdentityInfo {
            name: "Self Test Signer".into(),
            organization: "Folitecta".into(),
            ..Default::default()
        },
        folitecta_core::digsig::KeyKind::EcP256,
        1,
        "selftest-password",
    );
    let signed_tab =
        match created.and_then(|stored| crate::commands::signatures::load_id(&stored, "selftest-password")) {
            Ok(id) => {
                let opts = folitecta_core::digsig::SignOptions {
                    reason: "Self-test".into(),
                    ..Default::default()
                };
                st.read_active(|t| t.doc.sign(&id, &opts)).and_then(|r| r.ok())
            }
            Err(e) => {
                cx.record("digital ID created", false, e);
                None
            }
        };
    if let Some(bytes) = signed_tab {
        let path = dir.join("signed.pdf");
        let _ = std::fs::write(&path, &bytes);
        crate::files::open_path(st, path, None);
        pause(1200).await;
        {
            let mut ui = st.ui_mut();
            ui.show_left_pane = true;
            ui.left_pane = "signatures".into();
        }
        pause(800).await;
        let warn = eval("return document.querySelectorAll('.sig-item.warn').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        cx.record(
            "signed document: valid signature, identity not yet trusted",
            warn == 1,
            format!("{warn} item(s)"),
        );
        let field = crate::commands::signatures::signature_status(st)
            .first()
            .map(|s| s.field.clone())
            .unwrap_or_default();
        st.set_dialog(Dialog::SignatureDetails(field));
        // Wait for the dialog, press "trust", then wait for the pane to update.
        let mut clicked = false;
        for _ in 0..20 {
            pause(150).await;
            clicked = eval(
                "const b = [...document.querySelectorAll('.dialog .row .tbtn.primary')][0]; if (b) { b.click(); return true; } return false;",
            )
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
            if clicked {
                break;
            }
        }
        pause(500).await;
        st.set_dialog(Dialog::None);
        let mut ok = 0;
        for _ in 0..20 {
            pause(150).await;
            ok = eval("return document.querySelectorAll('.sig-item.ok').length;")
                .await
                .ok()
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if ok == 1 {
                break;
            }
        }
        let _ = clicked;
        cx.record(
            "trusting the signer's certificate",
            ok == 1,
            format!("{ok} trusted"),
        );
        let _ = capture_window(&dir.join("interaction-signature.png"));
        // Close the signed document again.
        if let Some(i) = st.active_index() {
            st.docs_mut().remove(i);
            st.set_active(0);
        }
        st.ui_mut().left_pane = "thumbnails".into();
        pause(500).await;
    } else {
        cx.record("document signed", false, String::new());
    }

    // 24. Handwritten signature: draw it in the library, place it with a click, apply it.
    st.set_dialog(Dialog::SignatureLibrary);
    pause(600).await;
    let _ = eval(
        "const p = document.querySelector('.sign-pad'); if (p) { const r = p.getBoundingClientRect(); const ev = (t, x, y) => p.dispatchEvent(new MouseEvent(t, { bubbles: true, button: 0, clientX: r.left + x, clientY: r.top + y })); ev('mousedown', 30, 90); for (let i = 1; i <= 12; i++) ev('mousemove', 30 + i * 25, 90 - Math.sin(i) * 40); ev('mouseup', 330, 70); }",
    )
    .await;
    pause(400).await;
    let _ = eval("const b = [...document.querySelectorAll('.dialog .tbtn.primary')].find(b => !b.closest('.sig-card') && !b.closest('.dialog-foot')); if (b) b.click();").await;
    pause(500).await;
    let saved = crate::commands::signs::library();
    cx.record(
        "signature drawn and saved in the library",
        saved.len() == 1 && saved[0].strokes.first().map(|s| s.len() > 5).unwrap_or(false),
        format!("{} saved", saved.len()),
    );
    let _ = capture_window(&dir.join("interaction-signature-library.png"));
    let _ = eval("const b = document.querySelector('.sig-card .tbtn.primary'); if (b) b.click();").await;
    pause(400).await;
    let before = st.read_active(|t| t.doc.signature_marks().len()).unwrap_or(0);
    let _ = pointer_path(0, z, &[(300.0, 520.0), (300.0, 520.0)], false).await;
    pause(600).await;
    let after = st.read_active(|t| t.doc.signature_marks().len()).unwrap_or(0);
    cx.record(
        "signature placed with a click",
        after == before + 1,
        format!("{before} -> {after}"),
    );
    crate::commands::dispatch(st, "signs.applyAll");
    pause(400).await;
    let left = st.read_active(|t| t.doc.signature_marks().len()).unwrap_or(9);
    cx.record(
        "signatures applied to the page",
        left == 0,
        format!("{left} left"),
    );
    st.set_tool(Tool::Hand);
}
