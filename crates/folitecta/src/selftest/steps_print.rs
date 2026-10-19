//! Interaction step: the print window. Opening it prints nothing; pages
//! 1-6 two to a sheet make three sides; as a booklet they make four sides on
//! two sheets, the second side the back of the first sheet; two collated
//! copies go to the fake printer (never a real one), and the print file it
//! received reopens with four A4 sides, no annotations, a blank half then
//! page 1 on the first side. The settings are remembered.

use std::sync::Arc;

use super::*;

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

async fn click(selector: &str) {
    let _ = eval(&format!("document.querySelector({})?.click();", js_str(selector))).await;
    pause(250).await;
}

async fn type_in(selector: &str, text: &str) {
    let _ = eval(&format!(
        "const i = document.querySelector({}); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        js_str(selector),
        js_str(text)
    ))
    .await;
    pause(250).await;
}

async fn choose(selector: &str, value: &str) {
    let _ = eval(&format!(
        "const s = document.querySelector({}); if (s) {{ s.value = {}; s.dispatchEvent(new Event('change', {{ bubbles: true }})); }}",
        js_str(selector),
        js_str(value)
    ))
    .await;
    pause(250).await;
}

/// The number of sides the window shows (-1 when it is not shown).
async fn sides_shown() -> i64 {
    eval("const e = document.querySelector('.print-sheet-count'); return e ? parseInt(e.dataset.count, 10) : -1;")
        .await
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(-1)
}

/// What the window says besides the preview (its hints, warnings and the
/// state of its button), for the details of a failed check.
async fn window_notes() -> String {
    eval(
        "const t = s => [...document.querySelectorAll(s)].map(e => e.innerText.trim()).filter(x => x).join(' | '); \
         const b = document.querySelector('.dialog-foot .tbtn.primary'); \
         const w = document.querySelector('.print-window'); \
         return 'wait: ' + t('.print-wait') + '; hints: ' + t('.print-hint') + '; warnings: ' + t('.print-warning') + '; error: ' + t('.print-error') + '; button disabled: ' + (b ? b.disabled : 'none') + '; window: ' + (w ? JSON.stringify(w.dataset) : 'none');",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default()
}

async fn text_of(selector: &str) -> String {
    eval(&format!(
        "const e = document.querySelector({}); return e ? e.innerText : '';",
        js_str(selector)
    ))
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default()
}

/// Wait (at most `secs` seconds) until the window shows `want` sides.
async fn wait_sides(want: i64, secs: u64) -> i64 {
    let mut got = sides_shown().await;
    for _ in 0..secs * 5 {
        if got == want {
            break;
        }
        pause(200).await;
        got = sides_shown().await;
    }
    got
}

/// Wait until the preview shows a drawn sheet.
async fn wait_preview(secs: u64) -> bool {
    for _ in 0..secs * 5 {
        let drawn = eval("const i = document.querySelector('.print-sheet-img'); return !!(i && i.complete && i.naturalWidth > 0);")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if drawn {
            return true;
        }
        pause(200).await;
    }
    false
}

/// Light pixels of a band of a rendered page (rows from `y0` to `y1`, as fractions of the height).
fn light_share(img: &image::RgbaImage, y0: f32, y1: f32) -> f32 {
    let (w, h) = img.dimensions();
    let (a, b) = ((h as f32 * y0) as u32, (h as f32 * y1) as u32);
    let mut light = 0u32;
    let mut all = 0u32;
    for y in a..b.min(h) {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            all += 1;
            if p[0] > 245 && p[1] > 245 && p[2] > 245 {
                light += 1;
            }
        }
    }
    light as f32 / all.max(1) as f32
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(fake) = crate::printing::fake() else {
        cx.record("print: the fake printer is in use", false, String::new());
        return;
    };
    fake.clear();
    st.ui_mut().values.remove(crate::ui::features::print::PREFS_KEY);
    st.set_dialog(Dialog::None);
    pause(200).await;
    // 1. Opening the window prints nothing; the preview is drawn.
    crate::commands::dispatch(st, "print");
    let drawn = wait_preview(20).await;
    let first_sides = sides_shown().await;
    let notes = window_notes().await;
    cx.record(
        "print: the window opens with a drawn preview and prints nothing",
        drawn && first_sides > 0 && fake.jobs().is_empty(),
        format!(
            "preview drawn {drawn}, {first_sides} sides, {} jobs; {notes}",
            fake.jobs().len()
        ),
    );
    // 2. Pages 1-6 on the duplex test printer, A4, two pages on each side.
    choose(".print-printer", crate::printing::fake::DUPLEX).await;
    click(".print-range-radio").await;
    type_in(".print-range", "1-6").await;
    choose(".print-paper", "iso_a4_210x297mm").await;
    click(".print-mode-multiple").await;
    choose(".print-per-sheet", "2").await;
    let two_up = wait_sides(3, 10).await;
    let _ = capture_window(&cx.dir.join("interaction-print-2up.png"));
    cx.record(
        "print: pages 1-6 two to a side make 3 sides",
        two_up == 3,
        format!("{two_up} sides; {}", text_of(".print-summary").await),
    );
    // 3. A booklet: 4 sides on 2 sheets, the second side the back of sheet 1.
    click(".print-mode-booklet").await;
    let booklet = wait_sides(4, 10).await;
    let summary = text_of(".print-summary").await;
    click(".print-next").await;
    pause(300).await;
    let label = text_of(".print-sheet-count").await;
    let preview = wait_preview(15).await;
    let _ = capture_window(&cx.dir.join("interaction-print-booklet.png"));
    let lang = st.lang();
    let want_summary = crate::i18n::tf(
        lang,
        "dialog.print.summary",
        &[("pages", "6"), ("sides", "4"), ("sheets", "2")],
    );
    let want_label = format!(
        "{} ({})",
        crate::i18n::tf(lang, "dialog.print.side_label", &[("n", "2"), ("count", "4")]),
        crate::i18n::tf(lang, "dialog.print.sheet_back", &[("n", "1")])
    );
    cx.record(
        "print: the booklet has 4 sides on 2 sheets, side 2 the back of sheet 1",
        booklet == 4 && summary.contains(&want_summary) && label.trim() == want_label && preview,
        format!("{booklet} sides; {summary}; '{label}'; preview {preview}"),
    );
    // 4. Two collated copies to the fake printer.
    type_in("#print-copies", "2").await;
    click(".dialog-foot .tbtn.primary").await;
    let mut jobs = fake.jobs();
    for _ in 0..75 {
        if !jobs.is_empty() && !crate::commands::print::JOB.peek().running {
            break;
        }
        pause(200).await;
        jobs = fake.jobs();
    }
    let closed = matches!(*st.dialog.peek(), Dialog::None);
    let ticket = jobs.first().map(|j| j.ticket.clone());
    let ticket_ok = ticket.as_ref().is_some_and(|t| {
        t.copies == 2
            && t.collate
            && t.sides == folitecta_core::print::Sides::ShortEdge
            && t.media.as_ref().map(|m| m.key.as_str()) == Some("iso_a4_210x297mm")
            && t.printer == crate::printing::fake::DUPLEX
    });
    cx.record(
        "print: one job of 2 collated copies, short-edge, A4 reaches the fake printer",
        jobs.len() == 1 && ticket_ok && closed,
        format!("{} jobs, ticket {ticket:?}, window closed {closed}", jobs.len()),
    );
    // 5. The print file the printer received.
    let pdf: Option<Arc<Vec<u8>>> = jobs.first().map(|j| j.pdf.clone());
    let checked = match pdf {
        Some(pdf) => tokio::task::spawn_blocking(move || -> Result<String, String> {
            let doc =
                folitecta_core::Document::from_bytes(pdf.to_vec(), None, None).map_err(|e| e.to_string())?;
            if doc.page_count() != 4 {
                return Err(format!("{} sides", doc.page_count()));
            }
            for p in 0..4 {
                let size = doc.page_size(p).map_err(|e| e.to_string())?;
                let mb = size.media_box;
                if (mb[2] - 595.28).abs() > 0.05 || (mb[3] - 841.89).abs() > 0.05 {
                    return Err(format!("side {} is {mb:?}", p + 1));
                }
                let id = doc.page_id(p).map_err(|e| e.to_string())?;
                if doc
                    .inner()
                    .get_dictionary(id)
                    .map(|d| d.has(b"Annots"))
                    .unwrap_or(true)
                {
                    return Err(format!("side {} has annotations", p + 1));
                }
            }
            let img = folitecta_core::Renderer::new(pdf)
                .and_then(|r| r.render_rgba(0, 0.2, folitecta_core::RenderOpts::default()))
                .map_err(|e| e.to_string())?;
            // The left half of the turned side (the bottom of the paper) is the
            // blank page completing the booklet; the right half is page 1.
            let bottom = light_share(&img, 0.55, 0.95);
            let top = light_share(&img, 0.05, 0.45);
            if bottom < 0.99 || top > bottom - 0.02 {
                return Err(format!("light share: bottom {bottom:.3}, top {top:.3}"));
            }
            Ok(format!("4 A4 sides, bottom {bottom:.3} light, top {top:.3}"))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r),
        None => Err("no job".into()),
    };
    cx.record(
        "print: the job's print file reopens with 4 A4 sides, a blank half and page 1",
        checked.is_ok(),
        checked.unwrap_or_else(|e| e),
    );
    // 6. The settings are remembered.
    let saved = st.value(crate::ui::features::print::PREFS_KEY);
    cx.record(
        "print: the settings are remembered",
        saved.contains("\"booklet\"") && saved.contains(crate::printing::fake::DUPLEX),
        saved.chars().take(160).collect(),
    );
    st.set_dialog(Dialog::None);
    st.ui_mut().values.remove(crate::ui::features::print::PREFS_KEY);
    fake.clear();
    pause(300).await;
    portfolio(cx).await;
    owner_password(cx).await;
}

/// A document that opens without a password and forbids printing: the window
/// refuses it, a wrong owner password changes nothing, the right one allows
/// printing (for the tab too).
async fn owner_password(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let path = cx.dir.join("print-owner-only.pdf");
    let no_print = folitecta_core::SecurityPermissions {
        print: false,
        ..folitecta_core::SecurityPermissions::default()
    };
    let written = folitecta_core::Document::new_blank(2, 595.0, 842.0).and_then(|d| {
        d.save_encrypted(
            &path,
            "",
            "o",
            no_print,
            folitecta_core::EncryptionProfile::Aes256,
        )
    });
    if let Err(e) = written {
        cx.record(
            "print: a document whose owner password allows printing",
            false,
            e.to_string(),
        );
        return;
    }
    let before = st.active_index();
    let count = st.docs.read().len();
    crate::files::open_path(st, path.clone(), None);
    for _ in 0..30 {
        if st.docs.read().len() > count {
            break;
        }
        pause(200).await;
    }
    let tab = st.read_active(|t| t.id);
    crate::commands::dispatch(st, "print");
    let mut refused = false;
    for _ in 0..50 {
        refused = eval("const b = document.querySelector('.dialog-foot .tbtn.primary'); return !!(document.querySelector('.print-owner-pw') && b && b.disabled);")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if refused {
            break;
        }
        pause(200).await;
    }
    type_in(".print-owner-pw", "x").await;
    click(".print-owner-unlock").await;
    let wrong = eval("return !!document.querySelector('.print-owner-wrong');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    type_in(".print-owner-pw", "o").await;
    click(".print-owner-unlock").await;
    let mut allowed = false;
    for _ in 0..50 {
        allowed = eval("const b = document.querySelector('.dialog-foot .tbtn.primary'); return !document.querySelector('.print-owner-pw') && !!b && !b.disabled;")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if allowed {
            break;
        }
        pause(200).await;
    }
    let tab_allowed = tab.and_then(|id| {
        st.docs
            .read()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.doc.print_permission())
    }) == Some(folitecta_core::print::PrintPermission::Allowed);
    let notes = window_notes().await;
    cx.record(
        "print: a document without a user password refuses printing until its owner password is typed in the window",
        refused && wrong && allowed && tab_allowed,
        format!("refused {refused}, wrong password noticed {wrong}, allowed {allowed}, tab allowed {tab_allowed}; {notes}"),
    );
    st.set_dialog(Dialog::None);
    pause(200).await;
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
    if let Some(b) = before {
        st.set_active(b.min(st.docs.read().len().saturating_sub(1)));
    }
}

/// A portfolio prints its own PDF files (three pages in two files), not the
/// other documents open.
async fn portfolio(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = (|| -> folitecta_core::Result<folitecta_core::Document> {
        let one = folitecta_core::Document::new_blank(1, 595.0, 842.0)?;
        let two = folitecta_core::Document::new_blank(2, 595.0, 842.0)?;
        folitecta_core::Document::new_portfolio(
            vec![
                ("one.pdf".into(), one.bytes().to_vec()),
                ("two.pdf".into(), two.bytes().to_vec()),
                ("notes.txt".into(), b"not a PDF".to_vec()),
            ],
            "Print test files",
        )
    })();
    let Ok(doc) = made else {
        cx.record(
            "print: a portfolio of two PDF files",
            false,
            format!("{:?}", made.err()),
        );
        return;
    };
    let before = st.active_index();
    let id = st.alloc_id();
    st.docs_mut().push(DocTab::new(id, doc));
    let n = st.docs.read().len();
    st.set_active(n - 1);
    pause(600).await;
    crate::commands::dispatch(st, "view.portfolio.print");
    let sides = wait_sides(3, 20).await;
    let files = match &*st.dialog.peek() {
        Dialog::Print(crate::state::PrintTarget::Files { files, .. }) => files.len(),
        _ => 0,
    };
    let text = text_of(".print-docs").await;
    let want = crate::i18n::tf(
        st.lang(),
        "dialog.print.docs_summary",
        &[("docs", "2"), ("pages", "3")],
    );
    cx.record(
        "print: a portfolio prints its own two PDF files (3 pages), no other open document",
        sides == 3 && files == 2 && text.trim() == want,
        format!("{sides} sides, {files} files, '{text}'"),
    );
    st.set_dialog(Dialog::None);
    pause(200).await;
    let index = st.docs.read().iter().position(|t| t.id == id);
    if let Some(i) = index {
        st.docs_mut().remove(i);
    }
    if let Some(b) = before {
        st.set_active(b.min(st.docs.read().len().saturating_sub(1)));
    }
    pause(300).await;
}
