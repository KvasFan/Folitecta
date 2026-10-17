//! Interaction step 45: sizes changed by dragging — the width of the left
//! pane, the rows of the ribbon and the width of a ribbon group — the first
//! tab named Edit, and the names of the tools shown or kept in the tooltips.

use super::*;
use crate::ui::chrome::layout::{LEFT_W, RIBBON_ROWS, group_key};

/// Drag the handle matched by `selector` by (dx, dy) pixels, then release.
/// Returns false when the handle is missing.
async fn drag(selector: &str, dx: f64, dy: f64) -> bool {
    let js = format!(
        r#"
const h = document.querySelector({sel});
if (!h) return false;
const r = h.getBoundingClientRect();
const x = r.left + r.width / 2, y = r.top + r.height / 2;
h.dispatchEvent(new MouseEvent('mousedown', {{ bubbles: true, cancelable: true, button: 0, buttons: 1, clientX: x, clientY: y }}));
await new Promise(res => setTimeout(res, 250));
const shield = document.querySelector('.drag-shield');
if (!shield) return false;
for (let i = 1; i <= 4; i++) {{
  shield.dispatchEvent(new MouseEvent('mousemove', {{ bubbles: true, buttons: 1, clientX: x + {dx} * i / 4, clientY: y + {dy} * i / 4 }}));
  await new Promise(res => setTimeout(res, 60));
}}
shield.dispatchEvent(new MouseEvent('mouseup', {{ bubbles: true, clientX: x + {dx}, clientY: y + {dy} }}));
return true;
"#,
        sel = serde_json::to_string(selector).unwrap_or_default()
    );
    let ok = document::eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    pause(400).await;
    ok
}

async fn double_click(selector: &str) {
    let js = format!(
        "document.querySelector({})?.dispatchEvent(new MouseEvent('dblclick', {{ bubbles: true }}));",
        serde_json::to_string(selector).unwrap_or_default()
    );
    let _ = document::eval(&js).await;
    pause(400).await;
}

async fn number(js: impl AsRef<str>) -> f64 {
    document::eval(js.as_ref())
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    // 45. The first tab is Edit; the left pane, the ribbon and a group are
    // resized by dragging their edges and restored by a double click.
    st.set_ribbon_tab("home".into());
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "thumbnails".into();
    }
    pause(400).await;
    let tab = document::eval("return document.querySelector('.ribbon-tab.active')?.textContent || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let width =
        || number("return document.querySelector('.pane-panel')?.getBoundingClientRect().width || 0;");
    let before = width().await;
    let dragged = drag(".splitter.left-edge", 90.0, 0.0).await;
    let after = width().await;
    let saved = st.value(LEFT_W).parse::<f64>().unwrap_or(0.0);
    double_click(".splitter.left-edge").await;
    let restored = width().await;
    cx.record(
        "the left pane is widened by dragging its edge, restored by a double click",
        tab == crate::i18n::t(st.lang(), "tab.home")
            && dragged
            && (after - before - 90.0).abs() < 3.0
            && (saved - after).abs() < 3.0
            && (restored - before).abs() < 3.0
            && st.value(LEFT_W).is_empty(),
        format!(
            "tab {tab:?}, {before} -> {after} (saved {saved}) -> {restored} (saved {:?})",
            st.value(LEFT_W)
        ),
    );

    // Two rows more: a taller ribbon with more tools in every column.
    let height =
        || number("return document.querySelector('.ribbon-body')?.getBoundingClientRect().height || 0;");
    let shown = || number("return document.querySelectorAll('.ribbon-body .rbtn:not(.has-menu)').length;");
    let (h0, n0) = (height().await, shown().await);
    let dragged = drag(
        ".splitter.ribbon-edge",
        0.0,
        2.0 * crate::ui::chrome::ribbon_layout::ROW_H,
    )
    .await;
    let (h1, n1) = (height().await, shown().await);
    let rows = st.value(RIBBON_ROWS);
    let _ = capture_window(&dir.join("interaction-ribbon-rows.png"));
    double_click(".splitter.ribbon-edge").await;
    let h2 = height().await;
    cx.record(
        "the ribbon is made taller by dragging its bottom edge, restored by a double click",
        dragged && rows == "5" && h1 > h0 + 30.0 && n1 >= n0 && (h2 - h0).abs() < 2.0,
        format!("rows {rows:?}, height {h0} -> {h1} -> {h2}, tools {n0} -> {n1}"),
    );

    // The comment group widened until it shows every tool it has.
    let group = ".ribbon-group[data-group=\"comment\"]";
    let tools = |g: &str| {
        number(format!(
            "return document.querySelectorAll('{}' + ' .rbtn:not(.has-menu)').length;",
            g.replace('\'', "\\'")
        ))
    };
    let t0 = tools(group).await;
    let dragged = drag(&format!("{group} > .splitter.group-grip"), 2000.0, 0.0).await;
    let t1 = tools(group).await;
    let more_left = number(format!(
        "return document.querySelectorAll('{} .rbtn.has-menu').length;",
        group.replace('\'', "\\'")
    ))
    .await;
    let key = group_key("home", "comment");
    let chosen = st.value(&key);
    let _ = capture_window(&dir.join("interaction-ribbon-group.png"));
    double_click(&format!("{group} > .splitter.group-grip")).await;
    let t2 = tools(group).await;
    cx.record(
        "a ribbon group is widened to show all its tools, restored by a double click",
        dragged && t1 > t0 && more_left == 0.0 && !chosen.is_empty() && t2 == t0 && st.value(&key).is_empty(),
        format!("tools {t0} -> {t1} (more buttons {more_left}, chosen {chosen:?}) -> {t2}"),
    );

    // The tools are icons with their names in the tooltip; View › Tool Names
    // puts the names next to the icons, and fewer tools fit.
    let names = || number("return document.querySelectorAll('.ribbon-body .rbtn .lbl').length;");
    let untitled = number(
        "return [...document.querySelectorAll('.ribbon-body .rbtn')].filter(b => !b.title || !b.getAttribute('aria-label')).length;",
    )
    .await;
    let (icons, names0) = (shown().await, names().await);
    crate::commands::dispatch(st, "view.ribbonLabels");
    pause(500).await;
    let (labelled, names1) = (shown().await, names().await);
    let _ = capture_window(&dir.join("interaction-tool-names.png"));
    crate::commands::dispatch(st, "view.ribbonLabels");
    pause(500).await;
    let (back, names2) = (shown().await, names().await);
    cx.record(
        "the tools show their names in the tooltip, and next to their icons when chosen",
        untitled == 0.0
            && names0 == 0.0
            && names1 > 0.0
            && icons >= labelled
            && back == icons
            && names2 == 0.0
            && !crate::commands::option_on(st, "view.ribbonLabels"),
        format!("without tooltip {untitled}; tools {icons} (names {names0}) -> {labelled} (names {names1}) -> {back} (names {names2})"),
    );
}
