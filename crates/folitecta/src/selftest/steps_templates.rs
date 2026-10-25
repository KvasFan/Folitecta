//! Interaction step: the gallery of templates — every template shown with
//! the picture of its first page; the invoice made into a new document with
//! its fields; a month planner made with a double click on Letter paper
//! turned sideways, the month of today in its title.

use super::*;

async fn count(js: &str) -> i64 {
    eval(js).await.ok().and_then(|v| v.as_i64()).unwrap_or(-1)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let tabs = st.docs.read().len();
    crate::commands::dispatch(st, "newDocTemplate");
    pause(400).await;
    let cards = count("return document.querySelectorAll('.dialog .tpl-card').length;").await;
    // The pictures are drawn away from the interface: wait for them all.
    let mut pictures = 0;
    for _ in 0..60 {
        pictures =
            count("return document.querySelectorAll('.dialog .tpl-card img.tpl-picture').length;").await;
        if pictures == cards {
            break;
        }
        pause(250).await;
    }
    let _ = eval("document.querySelector('.dialog .tpl-card[data-template=invoice]')?.click();").await;
    pause(300).await;
    let chosen = eval("return document.querySelector('.dialog .tpl-card.on')?.dataset.template || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let _ = capture_window(&cx.dir.join("interaction-templates.png"));
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut invoice = (0, 0);
    for _ in 0..40 {
        pause(150).await;
        if st.docs.read().len() > tabs {
            invoice = st
                .read_active(|t| (t.doc.page_count(), t.doc.form_fields().len()))
                .unwrap_or_default();
            break;
        }
    }
    let invoice_tab = st.read_active(|t| t.id).unwrap_or(0);

    crate::commands::dispatch(st, "newDocTemplate");
    pause(400).await;
    let _ = eval(
        "const s = document.querySelector('.dialog .tpl-paper'); s.value = 'letter'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelectorAll('.dialog .check input')[0]?.click();").await;
    pause(200).await;
    let _ = eval(
        "document.querySelector('.dialog .tpl-card[data-template=month]')?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));",
    )
    .await;
    let mut month = (0.0f32, 0.0f32, String::new());
    for _ in 0..40 {
        pause(150).await;
        if st.docs.read().len() > tabs + 1 {
            month = st
                .read_active(|t| {
                    let size = t
                        .doc
                        .page_size(0)
                        .map(|s| (s.width, s.height))
                        .unwrap_or_default();
                    (size.0, size.1, t.doc.page_text(0))
                })
                .unwrap_or_default();
            break;
        }
    }
    let month_tab = st.read_active(|t| t.id).unwrap_or(0);
    let (year, m) = folitecta_core::templates::this_month();
    let name = folitecta_core::templates::word(
        &crate::commands::templates::labels(st.lang()),
        &format!("month_{m}"),
    );
    let dialog_closed = count("return document.querySelectorAll('.dialog .tpl-card').length;").await == 0;
    for id in [month_tab, invoice_tab] {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(200).await;
    }
    cx.record(
        "the gallery shows every template with its picture; the invoice opens as a form; a month planner made by a double click on Letter paper sideways",
        cards == 13
            && pictures == 13
            && chosen == "invoice"
            && invoice.0 == 1
            && invoice.1 > 40
            && (month.0 - 792.0).abs() < 1.0
            && (month.1 - 612.0).abs() < 1.0
            && month.2.contains(&format!("{name} {year}"))
            && dialog_closed,
        format!(
            "cards {cards}, pictures {pictures}, chosen {chosen:?}, invoice {invoice:?}, month {:?} {:?}, closed {dialog_closed}",
            (month.0, month.1),
            month.2.chars().take(60).collect::<String>()
        ),
    );
}
