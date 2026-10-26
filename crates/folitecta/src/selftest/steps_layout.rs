//! Interaction step 45: sizes changed by dragging — the width of the left
//! pane, the rows of the ribbon and the width of a ribbon group, whose edge
//! follows the pointer — the first tab named Edit, the short names of the
//! tools under their icons or hidden, and submenus that wait for the pointer.

use super::*;
use crate::ui::chrome::layout::{LEFT_W, group_key, ribbon_rows_key};
use crate::ui::chrome::ribbon_layout::row_height;

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
    let ok = eval(&js).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
    pause(400).await;
    ok
}

async fn double_click(selector: &str) {
    let js = format!(
        "document.querySelector({})?.dispatchEvent(new MouseEvent('dblclick', {{ bubbles: true }}));",
        serde_json::to_string(selector).unwrap_or_default()
    );
    let _ = eval(&js).await;
    pause(400).await;
}

async fn number(js: impl AsRef<str>) -> f64 {
    eval(js.as_ref())
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
    let tab = eval("return document.querySelector('.ribbon-tab.active')?.textContent || '';")
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

    // Two rows more: a taller ribbon with more tools in every column (rows
    // of tiles, the tool names being shown by default).
    let height =
        || number("return document.querySelector('.ribbon-body')?.getBoundingClientRect().height || 0;");
    let shown = || number("return document.querySelectorAll('.ribbon-body .rbtn:not(.has-menu)').length;");
    let (rows_key, default_rows, _) = ribbon_rows_key(true);
    let (h0, n0) = (height().await, shown().await);
    let dragged = drag(".splitter.ribbon-edge", 0.0, 2.0 * row_height(true)).await;
    let (h1, n1) = (height().await, shown().await);
    let rows = st.value(rows_key);
    let _ = capture_window(&dir.join("interaction-ribbon-rows.png"));
    double_click(".splitter.ribbon-edge").await;
    let h2 = height().await;
    cx.record(
        "the ribbon is made taller by dragging its bottom edge, restored by a double click",
        dragged
            && rows == (default_rows + 2).to_string()
            && h1 > h0 + 30.0
            && n1 >= n0
            && (h2 - h0).abs() < 2.0,
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
    // A short drag: the edge of the group stays near the pointer.
    let group_w = || {
        number(format!(
            "return document.querySelector('{}')?.getBoundingClientRect().width || 0;",
            group.replace('\'', "\\'")
        ))
    };
    let w0 = group_w().await;
    let nudged = drag(&format!("{group} > .splitter.group-grip"), 90.0, 0.0).await;
    let w1 = group_w().await;
    cx.record(
        "the edge of a ribbon group follows the pointer when it is dragged",
        nudged && (w1 - w0 - 90.0).abs() <= 45.0,
        format!("width {w0} -> {w1} for a drag of 90 px"),
    );
    double_click(&format!("{group} > .splitter.group-grip")).await;
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

    // The tools show a short name under their icons, the whole name in the
    // tooltip; View › Tool Names hides the names, and more tools fit.
    let names = || number("return document.querySelectorAll('.ribbon-body .rbtn .lbl').length;");
    let untitled = number(
        "return [...document.querySelectorAll('.ribbon-body .rbtn')].filter(b => !b.title || !b.getAttribute('aria-label')).length;",
    )
    .await;
    // Under the icon the short name, in the tooltip the whole one (Zoom + / Zoom In).
    let zoom_in = eval(
        "const b = document.querySelector('.ribbon-body .rbtn[data-cmd=\"zoomIn\"]'); return b ? [b.querySelector('.lbl')?.textContent || '', b.title].join('|') : '';",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default();
    let reg = folitecta_core::registry();
    let expected = reg
        .get("zoomIn")
        .map(|c| {
            (
                crate::i18n::short_label(st.lang(), c),
                crate::i18n::cmd_label(st.lang(), c),
            )
        })
        .unwrap_or_default();
    let short_ok = zoom_in.is_empty()
        || zoom_in
            .split_once('|')
            .is_some_and(|(lbl, tip)| lbl == expected.0 && tip.starts_with(&expected.1));
    let (tiles, names0) = (shown().await, names().await);
    crate::commands::dispatch(st, "view.ribbonLabels");
    pause(500).await;
    let (icons, names1) = (shown().await, names().await);
    let _ = capture_window(&dir.join("interaction-tool-names.png"));
    crate::commands::dispatch(st, "view.ribbonLabels");
    pause(500).await;
    let (back, names2) = (shown().await, names().await);
    cx.record(
        "the tools show short names under their icons (the whole name in the tooltip), hidden when chosen",
        untitled == 0.0
            && short_ok
            && names0 > 0.0
            && names1 == 0.0
            && icons >= tiles
            && back == tiles
            && names2 == names0
            && crate::commands::option_on(st, "view.ribbonLabels"),
        format!(
            "without tooltip {untitled}; zoom in {zoom_in:?} (expected {expected:?}); tools {tiles} (names {names0}) -> {icons} (names {names1}) -> {back} (names {names2})"
        ),
    );

    // Submenus wait for the pointer: crossing another row on the way into an
    // open submenu leaves it open, resting on that row closes it.
    st.close_menus();
    st.set_open_menu(Some("view".into()));
    pause(400).await;
    // The pointer enters an element: the first row with a submenu, the first
    // row without one, or the open submenu.
    let enter = |which: &'static str| async move {
        let js = format!(
            r#"
const rows = [...document.querySelectorAll('.menu-dropdown > .menu-item')];
const el = {which};
if (!el) return '';
const r = el.getBoundingClientRect();
el.dispatchEvent(new MouseEvent('mouseenter', {{ bubbles: false, clientX: r.left + 20, clientY: r.top + 8 }}));
return (el.querySelector('.label')?.textContent || 'submenu');
"#
        );
        eval(&js)
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    const SUB: &str = "rows.find(e => e.querySelector('.chev'))";
    const LEAF: &str = "rows.find(e => !e.querySelector('.chev'))";
    const OPEN: &str = "document.querySelector('.submenu')";
    let open_now = || st.submenus.peek().len();
    let sub_row = enter(SUB).await;
    pause(250).await;
    let opened = open_now();
    let drawn = number("return document.querySelectorAll('.submenu').length;").await;
    let leaf_row = enter(LEAF).await;
    pause(120).await;
    let crossing = open_now();
    enter(OPEN).await;
    pause(500).await;
    let kept = open_now();
    enter(LEAF).await;
    pause(650).await;
    let closed = open_now();
    st.close_menus();
    pause(200).await;
    cx.record(
        "a submenu stays open while the pointer crosses other rows towards it, and closes when it rests on one",
        !sub_row.is_empty() && opened == 1 && drawn == 1.0 && crossing == 1 && kept == 1 && closed == 0,
        format!(
            "rows {sub_row:?} / {leaf_row:?}: open {opened} (drawn {drawn}), crossing {crossing}, inside {kept}, resting {closed}"
        ),
    );

    // The menus are short: the commands used most at the top of a menu, the
    // others in submenus by subject (File › New holds From Template…).
    let lang = st.lang();
    let new_label = crate::i18n::t(lang, "menu.group.new");
    let template = folitecta_core::registry()
        .get("newDocTemplate")
        .map(|c| crate::i18n::cmd_label(lang, c))
        .unwrap_or_default();
    st.close_menus();
    st.set_open_menu(Some("file".into()));
    pause(400).await;
    let top = number("return document.querySelectorAll('.menu-dropdown > .menu-item').length;").await;
    let js = format!(
        r#"
const row = [...document.querySelectorAll('.menu-dropdown > .menu-item')].find(e => e.querySelector('.label')?.textContent === {label});
if (!row) return '';
const r = row.getBoundingClientRect();
row.dispatchEvent(new MouseEvent('click', {{ bubbles: true, clientX: r.left + 20, clientY: r.top + 8 }}));
return 'opened';
"#,
        label = serde_json::to_string(&new_label).unwrap_or_default()
    );
    let opened = eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    pause(400).await;
    let inside = eval("return [...document.querySelectorAll('.submenu .menu-item .label')].map(e => e.textContent).join('|');")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    st.close_menus();
    pause(200).await;
    cx.record(
        "the menus are short: File has its most used commands at the top, the others in submenus (New holds From Template…)",
        top > 0.0 && top <= 22.0 && opened == "opened" && inside.split('|').any(|l| l == template),
        format!("File rows {top}, New {opened:?}, inside {inside:?}"),
    );
}
