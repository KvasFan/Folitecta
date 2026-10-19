//! Interaction step 43: the split pages editor.

use super::*;
use crate::commands::split::SPLIT;

async fn count(selector: &str) -> u64 {
    eval(&format!("return document.querySelectorAll('{selector}').length;"))
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    // 43. Split pages: a horizontal and a vertical line, the first dragged,
    // the second typed in millimetres, other units, rulers, every line
    // cleared after a confirmation, a 2 × 2 grid, and the page cut in four.
    st.go_to_page(0);
    pause(300).await;
    let (pw, ph) = crate::commands::split::page_size(st, 0);
    crate::commands::dispatch(st, "splitPages.addHorizontal");
    crate::commands::dispatch(st, "splitPages.addVertical");
    pause(800).await;
    let open = *st.dialog.peek() == Dialog::SplitPages;
    let lines = SPLIT.peek().lines.clone();
    let middle = lines.len() == 2
        && lines[0].horizontal
        && (lines[0].pos - ph / 2.0).abs() < 1.0
        && !lines[1].horizontal
        && (lines[1].pos - pw / 2.0).abs() < 1.0;
    // Drag the horizontal line to a quarter of the page.
    let _ = eval(
        r#"
const c = document.querySelector('.split-canvas');
const l = c && c.querySelector('.split-line.h');
if (l) {
  const r = c.getBoundingClientRect(), lr = l.getBoundingClientRect();
  l.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, buttons: 1, clientX: r.left + r.width / 2, clientY: lr.top + lr.height / 2 }));
}
"#,
    )
    .await;
    pause(250).await;
    let _ = eval(
        r#"
const c = document.querySelector('.split-canvas');
const img = c && c.querySelector('img');
if (img) {
  const r = c.getBoundingClientRect();
  const at = (f) => ({ bubbles: true, cancelable: true, button: 0, buttons: 1, clientX: r.left + r.width / 3, clientY: r.top + r.height * f });
  img.dispatchEvent(new MouseEvent('mousemove', at(0.4)));
  img.dispatchEvent(new MouseEvent('mousemove', at(0.25)));
  img.dispatchEvent(new MouseEvent('mouseup', at(0.25)));
}
"#,
    )
    .await;
    pause(300).await;
    let dragged = SPLIT.peek().lines.first().map(|l| l.pos).unwrap_or(0.0);
    // Type the vertical line in millimetres.
    st.set_value("split.units", "mm");
    pause(200).await;
    let _ = eval(
        "const i = document.getElementById('split-line-1'); if (i) { i.value = '50'; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let typed = SPLIT.peek().lines.get(1).map(|l| l.pos).unwrap_or(0.0);
    cx.record(
        "split lines added, dragged and typed",
        open && middle && (dragged - ph * 0.25).abs() < 2.0 && (typed - 50.0 * 72.0 / 25.4).abs() < 0.5,
        format!("dragged {dragged:.1} of {ph:.1}, typed {typed:.2}"),
    );
    crate::commands::dispatch(st, "splitPages.units");
    crate::commands::dispatch(st, "splitPages.showRulers");
    pause(400).await;
    let cm = st.value("split.units") == "cm";
    let rulers = count(".split-ruler").await;
    let _ = capture_window(&dir.join("interaction-split-lines.png"));
    crate::commands::dispatch(st, "splitPages.clear");
    pause(300).await;
    let asked = count(".split-confirm").await == 1 && SPLIT.peek().lines.len() == 2;
    crate::commands::dispatch(st, "splitPages.clear");
    pause(300).await;
    let cleared = SPLIT.peek().lines.is_empty();
    cx.record(
        "split units, rulers and clearing after a confirmation",
        cm && rulers == 2 && asked && cleared,
        format!("cm {cm}, {rulers} rulers, asked {asked}, cleared {cleared}"),
    );
    // A 2 × 2 grid on this page only.
    let _ = eval(
        r#"
const ins = document.querySelectorAll('.split-grid input');
for (const i of ins) { i.value = '2'; i.dispatchEvent(new Event('input', { bubbles: true })); }
"#,
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.split-grid .tbtn')?.click();").await;
    pause(300).await;
    st.set_value("split.every", "current");
    pause(200).await;
    let grid = SPLIT.peek().lines.len();
    let before = st.page_count();
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let after = st.page_count();
    cx.record(
        "a page cut in four by a grid",
        grid == 2 && after == before + 3 && *st.dialog.peek() == Dialog::None,
        format!("{grid} lines, {before} -> {after} pages"),
    );
    if after == before + 3 {
        crate::commands::dispatch(st, "undo");
    }
    st.set_value("split.units", "");
    st.set_value("split.every", "");
    st.set_option("split.rulers", false);
    pause(300).await;
}
