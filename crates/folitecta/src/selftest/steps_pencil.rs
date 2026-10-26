//! The Pencil's quick panel: shown when the Pencil is chosen with its six
//! buttons, gone with another tool; green, a thickness of 5 and a colour of
//! the palette chosen there, and a stroke drawn with them.

use super::*;

async fn number(js: &str) -> u64 {
    eval(js).await.ok().and_then(|v| v.as_u64()).unwrap_or(0)
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

    // Another tool: the panel goes.
    st.set_tool(Tool::Hand);
    pause(400).await;
    let gone = number("return document.querySelectorAll('.pencil-panel').length;").await == 0;
    // The style of the next strokes back to the built-in one.
    crate::state::comment_styles::reset_default_style(st, "Ink");
    cx.record(
        "pencil panel: six buttons with the Pencil; green and a thickness of 5 chosen and drawn with; a colour of the palette; gone with another tool",
        buttons == 6
            && widths == crate::ui::features::pencil::WIDTHS.len() as u64
            && green == 1
            && chosen
            && drawn
            && stroke_ok
            && orange.is_some_and(|c| (c[0] - 1.0).abs() < 0.01 && (c[1] - 0.533).abs() < 0.01 && c[2] < 0.01)
            && palette_on == 1
            && gone,
        format!(
            "buttons {buttons}, widths {widths}, green {green}, chosen {chosen}, {r}, drawn {drawn}, stroke {drawn_style:?}, orange {orange:?}, palette {palette_on}, gone {gone}"
        ),
    );
}
