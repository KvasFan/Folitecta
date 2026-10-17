//! Interaction step 52: arranging comments — three boxes chosen with a
//! click and Shift-clicks, the Arrange tab shown, then aligned, spread
//! evenly, given the same size and brought to the front, still chosen
//! after every step.

use folitecta_core::Shape;
use folitecta_core::marks::pdf_rect_to_display;

use super::*;

/// Press on the comment whose title ends with `name`, with Shift or not.
async fn press(name: &str, shift: bool) {
    let js = format!(
        "const h = [...document.querySelectorAll('#canvas .annot-hit')].find(e => (e.title || '').endsWith({name})); if (h) {{ const r = h.getBoundingClientRect(); h.dispatchEvent(new MouseEvent('mousedown', {{ bubbles: true, shiftKey: {shift}, button: 0, clientX: r.left + 3, clientY: r.top + 3 }})); h.dispatchEvent(new MouseEvent('mouseup', {{ bubbles: true, shiftKey: {shift}, button: 0, clientX: r.left + 3, clientY: r.top + 3 }})); }}",
        name = serde_json::to_string(name).unwrap_or_default()
    );
    let _ = document::eval(&js).await;
    pause(300).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 52. Three boxes on the second page.
    let page = 1;
    let made = st
        .with_active(|t| {
            let size = t.doc.page_size(page).ok()?;
            let mut ids = Vec::new();
            for (i, r) in [
                [60.0, 500.0, 140.0, 540.0],
                [200.0, 560.0, 300.0, 620.0],
                [380.0, 520.0, 420.0, 560.0],
            ]
            .into_iter()
            .enumerate()
            {
                let pdf = folitecta_core::textops::display_rect_to_pdf(&size, r);
                let name = format!("arrange-{}", i + 1);
                ids.push(
                    t.doc
                        .add_shape(
                            page,
                            Shape::Square,
                            pdf,
                            [0.1, 0.4, 0.9],
                            None,
                            1.5,
                            "Reviewer",
                            &name,
                        )
                        .ok()?,
                );
            }
            t.refresh();
            Some(ids)
        })
        .flatten();
    if made.is_none() {
        cx.record("arranging comments", false, "the boxes could not be made".into());
        return;
    }
    st.set_tool(Tool::EditAnnots);
    st.go_to_page(page);
    pause(700).await;
    press("arrange-1", false).await;
    press("arrange-2", true).await;
    press("arrange-3", true).await;
    let chosen = st.read_active(|t| t.chosen_annots()).flatten();
    let tab = document::eval(
        "return [...document.querySelectorAll('.ribbon-tab.ctx')].map(b => b.textContent).join(',');",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default();
    st.set_ribbon_tab("ctx.arrange".into());
    pause(400).await;
    let _ = capture_window(&cx.dir.join("interaction-arrange.png"));
    // The boxes as the page shows them, in the order they were chosen.
    let boxes = || {
        st.read_active(|t| {
            let (p, list) = t.chosen_annots()?;
            let size = t.doc.page_size(p).ok()?;
            let annots = t.doc.annotations(p).ok()?;
            Some(
                list.iter()
                    .filter_map(|i| annots.iter().find(|a| a.index == *i))
                    .map(|a| pdf_rect_to_display(&size, a.rect))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .unwrap_or_default()
    };
    crate::commands::dispatch(st, "align.left");
    let left = boxes();
    let aligned = left.len() == 3 && left.iter().all(|r| (r[0] - left[0][0]).abs() < 0.6);
    crate::commands::dispatch(st, "distribVert.space");
    let mut spread = boxes();
    spread.sort_by(|a, b| a[1].total_cmp(&b[1]));
    let even =
        spread.len() == 3 && ((spread[1][1] - spread[0][3]) - (spread[2][1] - spread[1][3])).abs() < 0.6;
    crate::commands::dispatch(st, "makeSameSize");
    let sized = boxes();
    let same = sized.len() == 3
        && sized.iter().all(|r| {
            ((r[2] - r[0]) - (sized[0][2] - sized[0][0])).abs() < 0.6
                && ((r[3] - r[1]) - (sized[0][3] - sized[0][1])).abs() < 0.6
        });
    crate::commands::dispatch(st, "order.background");
    let after = st.read_active(|t| t.chosen_annots()).flatten();
    let still = after.as_ref().map(|(_, l)| l.len()) == Some(3)
        && after.as_ref().map(|(_, l)| l.contains(&0)) == Some(true);
    st.with_active(|t| t.set_chosen(page, &[]));
    st.set_tool(Tool::Hand);
    st.set_ribbon_tab("home".into());
    cx.record(
        "arranging comments: three chosen with Shift, aligned, spread evenly, sized alike and sent to the back",
        chosen.as_ref().map(|(_, l)| l.len()) == Some(3) && tab.contains(&crate::i18n::t(st.lang(), "tab.ctx.arrange")) && aligned && even && same && still,
        format!("chosen {chosen:?}, tabs {tab:?}, aligned {aligned}, even {even}, same {same}, after {after:?}"),
    );
}
