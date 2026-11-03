//! Interaction step: measures_ticks — the depth of a selected volume
//! measurement changed in the Properties pane changes the volume it shows;
//! the tick marks window saves a set of styles and fills its fields from it.

use folitecta_core::{AnnotStyle, MeasureScale};

use crate::commands::ticks;

use super::*;

async fn set_field(selector: &str, value: &str, event: &str) {
    let _ = eval(&format!(
        "const i = document.querySelector({}); if (i) {{ i.value = {}; i.dispatchEvent(new Event({}, {{ bubbles: true }})); }}",
        serde_json::to_string(selector).unwrap_or_default(),
        serde_json::to_string(value).unwrap_or_default(),
        serde_json::to_string(event).unwrap_or_default(),
    ))
    .await;
    pause(300).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(1, 595.0, 842.0).and_then(|mut d| {
        // 72 x 72 pt at 1 cm = 1 m is 6.45 m²; 2 m deep.
        let m = MeasureScale::from_ratio(1.0, "cm", 1.0, "m", 2);
        let square = [(250.0, 50.0), (322.0, 50.0), (322.0, 122.0), (250.0, 122.0)];
        d.add_volume_measure(0, &square, 2.0, &AnnotStyle::default(), &m, "t")?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record(
            "measures and ticks",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    let saved_sets = st.value("tick.sets");
    let right_was = st.ui.peek().show_right_pane;
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;

    // The depth, in the Properties pane of the selected volume.
    st.with_active(|t| t.selected_annot = Some((0, 0)));
    st.ui_mut().show_right_pane = true;
    pause(500).await;
    set_field(".right-pane .volume-depth", "3", "change").await;
    let (depth, shown) = st
        .read_active(|t| {
            (
                t.doc.volume_depth(0, 0),
                t.doc
                    .annotations(0)
                    .ok()
                    .and_then(|a| a.first().map(|a| a.contents.clone())),
            )
        })
        .unwrap_or_default();

    // A set of tick styles saved, then chosen again.
    st.set_value("tick.sets", "");
    crate::commands::dispatch(st, "tool.annot.tickMark");
    pause(400).await;
    set_field(".dialog .tick-size", "30", "input").await;
    set_field(".dialog .tick-set-name", "Large", "input").await;
    let _ = eval("document.querySelector('.dialog .tick-set-save')?.click();").await;
    pause(300).await;
    let stored = ticks::sets(st);
    set_field(".dialog .tick-size", "12", "input").await;
    set_field(".dialog .tick-set", "Large", "change").await;
    let size_back = eval("return document.querySelector('.dialog .tick-size')?.value ?? '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    st.set_dialog(Dialog::None);
    pause(200).await;

    st.set_value("tick.sets", saved_sets);
    st.ui_mut().show_right_pane = right_was;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let set_ok = stored.len() == 1 && stored[0].0 == "Large" && stored[0].1.size == 30.0;
    cx.record(
        "the depth of a volume changed in Properties shows the new volume; a set of tick styles saved and chosen again",
        depth == Some(3.0) && shown.as_deref().is_some_and(|s| s.starts_with("19.35")) && set_ok && size_back == "30",
        format!("depth {depth:?}, shown {shown:?}, sets {:?}, size back {size_back:?}", stored.iter().map(|s| &s.0).collect::<Vec<_>>()),
    );
}
