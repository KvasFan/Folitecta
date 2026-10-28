//! The quick panel of the drawing tools: shown when the Pencil is chosen
//! with its seven buttons; green, a thickness of 5 and a colour of the
//! palette chosen there, and a stroke drawn with them; a rectangle in blue
//! filled with green; the highlighter's own colours; the distance tool's
//! panel with the scale, two distances in a row; a pen's stroke keeping its
//! pressures, and an even one with the pressure off; gone with another tool.

use super::*;

async fn number(js: &str) -> u64 {
    eval(js).await.ok().and_then(|v| v.as_u64()).unwrap_or(0)
}

/// A stroke of a pen over page 0 through `PTS` (pixels of the page), pressing
/// lightly at the start and harder to the end: each point a pointer event of
/// a pen, then the mouse event the web view sends with it.
const PEN_JS: &str = r#"
const el = document.querySelector('#pg-0 .page-inner');
if (!el) return 'missing page';
const r = el.getBoundingClientRect();
const pts = PTS;
const at = (p) => ({ clientX: r.left + p[0], clientY: r.top + p[1] });
const pointer = (type, p, pressure, buttons) => el.dispatchEvent(new PointerEvent(type, Object.assign({ bubbles: true, cancelable: true, pointerType: 'pen', pointerId: 7, isPrimary: true, pressure, button: 0, buttons }, at(p))));
const mouse = (type, p, buttons) => el.dispatchEvent(new MouseEvent(type, Object.assign({ bubbles: true, cancelable: true, button: 0, buttons }, at(p))));
for (let i = 0; i < pts.length; i++) {
  const pressure = 0.05 + 0.95 * i / (pts.length - 1);
  pointer(i === 0 ? 'pointerdown' : 'pointermove', pts[i], pressure, 1);
  mouse(i === 0 ? 'mousedown' : 'mousemove', pts[i], 1);
}
const end = pts[pts.length - 1];
pointer('pointerup', end, 0, 0);
mouse('mouseup', end, 0);
return 'pen';
"#;

/// The pressures kept with the one comment added to page 0 after `before`
/// comments (its first path), if it has them.
fn last_pressures(st: AppState, before: usize) -> Option<Vec<f32>> {
    st.read_active(|t| {
        let after = t.doc.annotations(0).ok()?;
        if after.len() != before + 1 {
            return None;
        }
        let d = t.doc.inner().get_dictionary(after.last()?.id?).ok()?;
        let paths = d.get(b"FolitectaPressure").ok()?.as_array().ok()?;
        let first = paths.first()?.as_array().ok()?;
        Some(first.iter().filter_map(|v| v.as_float().ok()).collect())
    })
    .flatten()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, z) = (cx.st, cx.z);
    st.go_to_page(0);
    crate::commands::dispatch(st, "tool.annot.pencil");
    pause(500).await;
    let buttons = number(
        "return document.querySelectorAll('.pencil-panel > .pencil-btn, .pencil-panel > .pencil-width-wrap > .pencil-btn').length;",
    )
    .await;
    // Green, then a thickness of 5 from the thicknesses.
    let _ = eval("document.querySelector('.pencil-panel [data-colour=green]')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.pencil-panel .pencil-width')?.click();").await;
    pause(300).await;
    let widths = number("return document.querySelectorAll('.pencil-widths .pencil-btn').length;").await;
    let _ = eval("document.querySelector('.pencil-widths [data-width=\"5\"]')?.click();").await;
    pause(300).await;
    let green =
        number("return document.querySelectorAll('.pencil-panel [data-colour=green].active').length;").await;
    let style = crate::state::comment_styles::default_style(st, "Ink");
    let chosen = style.color == Some([0.0, 0.6, 0.2]) && style.width == 5.0;

    let before = annots_on(st, 0).len();
    let wave: Vec<(f32, f32)> = (0..16)
        .map(|i| (140.0 + i as f32 * 8.0, 520.0 + ((i as f32) * 0.8).sin() * 15.0))
        .collect();
    let r = pointer_path(0, z, &wave, false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    let drawn = after.len() == before + 1 && after.last().is_some_and(|a| a.subtype == "Ink");
    let drawn_style = after.last().and_then(|a| {
        st.read_active(|t| t.doc.annotation_style(0, a.index).ok())
            .flatten()
    });
    let stroke_ok = drawn_style
        .as_ref()
        .is_some_and(|s| s.width == 5.0 && s.color.is_some_and(|c| (c[1] - 0.6).abs() < 0.01 && c[0] < 0.01));

    // A colour of the palette.
    let _ = eval(
        "const i = document.querySelector('.pencil-panel .pencil-palette input'); if (i) { i.value = '#ff8800'; i.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let orange = crate::state::comment_styles::default_style(st, "Ink").color;
    let palette_on =
        number("return document.querySelectorAll('.pencil-panel .pencil-palette.active').length;").await;
    let _ = capture_window(&cx.dir.join("interaction-pencil-panel.png"));

    // The rectangle: its own panel, with a fill; blue filled with green.
    st.set_tool(Tool::Rect);
    pause(400).await;
    let rect_buttons = number(
        "return document.querySelectorAll('.quick-panel[data-tool=Square] > .pencil-btn, .quick-panel[data-tool=Square] > .pencil-width-wrap > .pencil-btn').length;",
    )
    .await;
    let _ = eval("document.querySelector('.quick-panel [data-colour=blue]')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.quick-panel .pencil-fill')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.quick-panel [data-fill=green]')?.click();").await;
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r2 = pointer_path(0, z, &line((150.0, 640.0), (260.0, 700.0), 6), false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    let rect_style = (after.len() == before + 1)
        .then(|| {
            after.last().and_then(|a| {
                st.read_active(|t| t.doc.annotation_style(0, a.index).ok())
                    .flatten()
            })
        })
        .flatten();
    let rect_ok = rect_style.as_ref().is_some_and(|s| {
        s.color.is_some_and(|c| (c[2] - 0.9).abs() < 0.01 && c[0] < 0.11)
            && s.fill.is_some_and(|f| (f[1] - 0.6).abs() < 0.01 && f[0] < 0.01)
    });
    // The free highlighter: its own colours.
    st.set_tool(Tool::FreeHighlight);
    pause(400).await;
    let marker = number(
        "return document.querySelectorAll('.quick-panel[data-tool=FreeHighlight] [data-colour=yellow], .quick-panel[data-tool=FreeHighlight] [data-colour=pink]').length;",
    )
    .await;
    let marker_fill = number("return document.querySelectorAll('.quick-panel .pencil-fill').length;").await;

    // The distance tool: its panel shows the scale and opens it; blue chosen
    // there, two distances drawn one after the other without choosing again.
    crate::commands::dispatch(st, "tool.annot.distance");
    pause(400).await;
    let scale_text = eval(
        "return document.querySelector('.quick-panel[data-tool=Distance] .pencil-scale')?.textContent || '';",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default();
    let _ =
        eval("document.querySelector('.quick-panel[data-tool=Distance] [data-colour=blue]')?.click();").await;
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r3 = pointer_path(0, z, &line((300.0, 560.0), (372.0, 560.0), 5), false).await;
    pause(500).await;
    let _ = pointer_path(0, z, &line((300.0, 590.0), (408.0, 590.0), 5), false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    let measures: Vec<String> = after.iter().skip(before).map(|a| a.contents.clone()).collect();
    let measure_style = after.last().and_then(|a| {
        st.read_active(|t| t.doc.annotation_style(0, a.index).ok())
            .flatten()
    });
    let measure_blue = measure_style
        .as_ref()
        .is_some_and(|s| s.color.is_some_and(|c| (c[2] - 0.9).abs() < 0.01 && c[0] < 0.11));
    let distance_kept = (st.tool)() == Tool::Distance;
    let _ = eval("document.querySelector('.quick-panel .pencil-scale')?.click();").await;
    pause(400).await;
    let scale_dialog = matches!(*st.dialog.peek(), crate::state::Dialog::Calibrate);
    st.set_dialog(crate::state::Dialog::None);
    pause(200).await;

    // The pen: its pressure on by default in the Pencil's panel; a stroke
    // drawn with a pen pressing lightly then hard keeps its pressures.
    crate::commands::dispatch(st, "tool.annot.pencil");
    pause(400).await;
    let pressure_btn = number(
        "return document.querySelectorAll('.quick-panel[data-tool=Ink] .pencil-pressure.active').length;",
    )
    .await;
    let pen_at = |y: f32| -> String {
        let pts: Vec<String> = line((140.0, y), (320.0, y), 18)
            .iter()
            .map(|(x, y)| format!("[{},{}]", x * z, y * z))
            .collect();
        PEN_JS.replace("PTS", &format!("[{}]", pts.join(",")))
    };
    let before = annots_on(st, 0).len();
    let r4 = eval(&pen_at(470.0))
        .await
        .map(|v| v.to_string())
        .unwrap_or_else(|e| format!("eval error: {e}"));
    pause(600).await;
    let kept = last_pressures(st, before);
    let pressed = kept.as_ref().is_some_and(|k| {
        k.len() > 4 && k.first().is_some_and(|p| *p < 0.35) && k.last().is_some_and(|p| *p > 0.65)
    });
    let _ = capture_window(&cx.dir.join("interaction-pen-pressure.png"));
    // Turned off: the pen draws an even line again.
    let _ = eval("document.querySelector('.quick-panel .pencil-pressure')?.click();").await;
    pause(300).await;
    let off = !crate::ui::features::quick_panel::pressure_on(st);
    let before = annots_on(st, 0).len();
    let _ = eval(&pen_at(440.0)).await;
    pause(600).await;
    let even = annots_on(st, 0).len() == before + 1 && last_pressures(st, before).is_none();
    st.set_value("ink.pressure", "1");

    // Another tool: the panel goes.
    st.set_tool(Tool::Hand);
    pause(400).await;
    let gone = number("return document.querySelectorAll('.pencil-panel').length;").await == 0;
    // The styles of the next comments back to the built-in ones.
    for kind in ["Ink", "Square", "Distance"] {
        crate::state::comment_styles::reset_default_style(st, kind);
    }
    cx.record(
        "quick panel of the measures: the scale shown and opened; blue chosen; two distances one after the other",
        scale_text.contains("1 mm = 1 mm")
            && measures.len() == 2
            && measures.iter().all(|m| m.ends_with("mm"))
            && measure_blue
            && distance_kept
            && scale_dialog,
        format!(
            "scale {scale_text:?}, {r3}, measures {measures:?}, style {measure_style:?}, kept {distance_kept}, dialog {scale_dialog}"
        ),
    );
    cx.record(
        "pen pressure: on in the Pencil's panel; a stroke thicker where the pen pressed harder; off, an even line",
        pressure_btn == 1 && pressed && off && even,
        format!("button {pressure_btn}, {r4}, pressures {kept:?}, off {off}, even {even}"),
    );
    cx.record(
        "quick panel: seven buttons with the Pencil; green and a thickness of 5 chosen and drawn with; a colour of the palette; a blue rectangle filled with green; the highlighter's colours; gone with another tool",
        buttons == 7
            && widths == crate::ui::features::quick_panel::WIDTHS.len() as u64
            && green == 1
            && chosen
            && drawn
            && stroke_ok
            && orange.is_some_and(|c| (c[0] - 1.0).abs() < 0.01 && (c[1] - 0.533).abs() < 0.01 && c[2] < 0.01)
            && palette_on == 1
            && rect_buttons == 7
            && rect_ok
            && marker == 2
            && marker_fill == 0
            && gone,
        format!(
            "buttons {buttons}, widths {widths}, green {green}, chosen {chosen}, {r}, drawn {drawn}, stroke {drawn_style:?}, orange {orange:?}, palette {palette_on}; rectangle buttons {rect_buttons}, {r2}, style {rect_style:?}; highlighter colours {marker}, fill {marker_fill}; gone {gone}"
        ),
    );
}
