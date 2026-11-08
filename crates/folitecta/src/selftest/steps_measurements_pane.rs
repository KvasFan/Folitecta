//! Interaction step: measurements_pane — two distances measured on a page:
//! the Measurements pane lists them under Distances with their total; a
//! click on one chooses it on its page.

use folitecta_core::{AnnotStyle, MeasureKind, MeasureScale};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(1, 400.0, 400.0).and_then(|mut d| {
        let mm = MeasureScale::default();
        let style = AnnotStyle::default();
        d.add_measurement(
            0,
            MeasureKind::Distance,
            &[(100.0, 100.0), (172.0, 100.0)],
            &style,
            &mm,
            "t",
        )?;
        d.add_measurement(
            0,
            MeasureKind::Distance,
            &[(100.0, 150.0), (136.0, 150.0)],
            &style,
            &mm,
            "t",
        )?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record(
            "measurements pane",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "view.measurements");
    pause(500).await;
    let text = |js: &'static str| async move {
        eval(js)
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let items = text("return String(document.querySelectorAll('.measure-item').length);").await;
    let total = text("return document.querySelector('.measure-total')?.textContent ?? '';").await;
    let _ = text("document.querySelectorAll('.measure-item')[1]?.click(); return '';").await;
    pause(300).await;
    let chosen = st.read_active(|t| t.selected_annot).flatten();
    crate::commands::dispatch(st, "view.measurements");
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "measurements pane: two distances listed with their total; a click chooses one",
        items == "2" && total.contains("38.1 mm") && chosen == Some((0, 1)),
        format!("items {items}, total {total:?}, chosen {chosen:?}"),
    );
}
