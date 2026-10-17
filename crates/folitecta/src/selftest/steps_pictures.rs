//! Interaction step 42: pictures selected on the page, edited straight away,
//! in the picture editor and in another app.

use super::*;
use crate::commands::pictures::{EDITOR, EXTERNAL, edit_outside, external_changed};

async fn count(selector: &str) -> u64 {
    document::eval(&format!("return document.querySelectorAll('{selector}').length;"))
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Wait until the editor has finished its work.
async fn settle() {
    for _ in 0..60 {
        pause(100).await;
        if !EDITOR.peek().busy {
            break;
        }
    }
    pause(150).await;
}

/// The picture added by this step: its index on page 0 and its description.
fn ours(st: AppState, object: Option<(u32, u16)>) -> Option<folitecta_core::pictures::PageImage> {
    st.read_active(|t| {
        let list = t.doc.page_images(0).ok()?;
        match object {
            Some(o) => list.iter().find(|im| im.object == o).cloned(),
            None => list.last().cloned(),
        }
    })
    .flatten()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    // 42. Pictures: one placed on the first page, selected with the pictures
    // tool, turned from its toolbar, edited in the editor (mirrored, gray,
    // a step undone, sharpened, cropped by dragging) and edited in another app.
    let px = image::RgbaImage::from_fn(120, 60, |x, y| {
        if x < 60 {
            if y % 12 < 2 {
                image::Rgba([20, 20, 20, 255])
            } else {
                image::Rgba([250, 250, 250, 255])
            }
        } else {
            image::Rgba([(x * 2) as u8, 90, 200, 255])
        }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = px.write_to(&mut png, image::ImageFormat::Png);
    st.go_to_page(0);
    let added = st
        .with_active(|t| {
            t.doc
                .add_image(0, png.into_inner(), Some([100.0, 400.0, 340.0, 520.0]))
                .is_ok()
        })
        .unwrap_or(false);
    let first = ours(st, None);
    let object = first.as_ref().map(|im| im.object);
    crate::commands::dispatch(st, "tool.editContent.images");
    pause(600).await;
    let boxes = count("#pg-0 .image-box").await;
    cx.record(
        "pictures tool outlines the pictures",
        added && boxes >= 1 && (st.tool)() == Tool::EditImages,
        format!("{boxes} boxes"),
    );
    // Select it (the last box is the picture just added) and turn it.
    let _ = document::eval(
        "const b = [...document.querySelectorAll('#pg-0 .image-box')].pop(); if (b) b.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));",
    )
    .await;
    pause(400).await;
    let bar = count(".picture-toolbar").await;
    let rotate_title = crate::i18n::cmd_label(
        st.lang(),
        folitecta_core::registry().get("imageEditor.rotate90CW").unwrap(),
    );
    let _ = document::eval(&format!(
        "const b = [...document.querySelectorAll('.picture-toolbar button')].find(e => e.title === {}); if (b) b.click();",
        serde_json::to_string(&rotate_title).unwrap_or_default()
    ))
    .await;
    let mut turned = None;
    for _ in 0..40 {
        pause(150).await;
        turned = ours(st, object).filter(|im| (im.width, im.height) == (60, 120));
        if turned.is_some() {
            break;
        }
    }
    let rect_ok = match (&first, &turned) {
        (Some(a), Some(b)) => {
            let (aw, ah) = (a.rect[2] - a.rect[0], a.rect[3] - a.rect[1]);
            let (bw, bh) = (b.rect[2] - b.rect[0], b.rect[3] - b.rect[1]);
            (aw - bh).abs() < 0.5 && (ah - bw).abs() < 0.5
        }
        _ => false,
    };
    let _ = capture_window(&dir.join("interaction-pictures-tool.png"));
    cx.record(
        "a picture turned from its toolbar keeps its centre",
        bar == 1 && rect_ok,
        format!("{bar} toolbar, {turned:?}"),
    );

    // The editor, opened with a double click.
    let _ = document::eval(
        "const b = [...document.querySelectorAll('#pg-0 .image-box')].pop(); if (b) b.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));",
    )
    .await;
    let mut shown = false;
    for _ in 0..40 {
        pause(150).await;
        shown = document::eval("const i = document.querySelector('.imged-img'); return !!(i && i.complete && i.naturalWidth > 0);")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if shown {
            break;
        }
    }
    let size = EDITOR.peek().size;
    cx.record(
        "the picture editor shows the picture",
        shown && *st.dialog.peek() == Dialog::ImageEditor && size == (60, 120),
        format!("{size:?}"),
    );
    crate::commands::dispatch(st, "imageEditor.flipHorizontal");
    settle().await;
    crate::commands::dispatch(st, "imageEditor.convert.8bpp_grayscale");
    settle().await;
    let gray = EDITOR.peek().format == Some(folitecta_core::imaging::Format::Gray);
    crate::commands::pictures::undo_in_editor();
    settle().await;
    let undone = EDITOR.peek().format == Some(folitecta_core::imaging::Format::Rgb);
    crate::commands::dispatch(st, "imageEditor.grayscale");
    settle().await;
    crate::commands::dispatch(st, "imageEditor.filters.rcm");
    pause(300).await;
    let filters = count(".imged-menu button").await;
    crate::commands::dispatch(st, "imageEditor.filters.sharpen");
    settle().await;
    // Crop by dragging over the middle of the picture.
    crate::commands::dispatch(st, "imageEditor.crop");
    pause(300).await;
    let _ = document::eval(
        r#"
const c = document.querySelector('.imged-canvas');
const img = c && c.querySelector('img');
if (img) {
  const r = c.getBoundingClientRect();
  const at = (fx, fy) => ({ bubbles: true, cancelable: true, button: 0, buttons: 1, clientX: r.left + r.width * fx, clientY: r.top + r.height * fy });
  img.dispatchEvent(new MouseEvent('mousedown', at(0.25, 0.25)));
  img.dispatchEvent(new MouseEvent('mousemove', at(0.5, 0.5)));
  img.dispatchEvent(new MouseEvent('mousemove', at(0.75, 0.75)));
  img.dispatchEvent(new MouseEvent('mouseup', at(0.75, 0.75)));
}
"#,
    )
    .await;
    pause(300).await;
    let area = EDITOR.peek().crop;
    let _ = capture_window(&dir.join("interaction-picture-editor-crop.png"));
    crate::commands::dispatch(st, "imageEditor.crop");
    settle().await;
    let cropped = EDITOR.peek().size;
    crate::commands::dispatch(st, "imageEditor.zoomIn");
    pause(200).await;
    let zoomed = EDITOR.peek().zoom.is_some();
    crate::commands::dispatch(st, "imageEditor.fitVisible");
    pause(200).await;
    cx.record(
        "editor: mirror, gray, undo, filters, crop and zoom",
        gray && undone
            && filters == 10
            && area.is_some()
            && cropped.0 < 60
            && cropped.1 < 120
            && cropped.0 >= 20
            && zoomed
            && EDITOR.peek().zoom.is_none(),
        format!("gray {gray}, undone {undone}, {filters} filters, area {area:?}, cropped {cropped:?}"),
    );
    // The editor at the smallest window size.
    set_size(960.0, 620.0);
    pause(700).await;
    cx.failures += check(st, dir, "960x620-picture-editor", cx.log).await;
    set_size(1280.0, 800.0);
    pause(500).await;
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let applied = ours(st, object);
    cx.record(
        "the edited picture goes on the page",
        applied.as_ref().is_some_and(|im| {
            (im.width, im.height) == cropped && im.colors == folitecta_core::pictures::Colors::Gray
        }) && *st.dialog.peek() == Dialog::None,
        format!("{applied:?}"),
    );

    // Another app: the picture is written to a file, changed there and read back.
    let index = applied.as_ref().map(|im| im.index).unwrap_or(0);
    st.with_active(|t| t.image_sel = Some((0, index)));
    edit_outside(st, false);
    pause(400).await;
    let path = EXTERNAL.peek().as_ref().map(|x| x.path.clone());
    let wrote = path.as_ref().is_some_and(|p| p.exists());
    pause(1100).await;
    if let Some(p) = &path {
        let _ = image::RgbaImage::from_pixel(30, 30, image::Rgba([200, 30, 30, 255])).save(p);
    }
    pause(300).await;
    let changed = external_changed();
    let _ = capture_window(&dir.join("interaction-picture-outside.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let updated = ours(st, object);
    st.set_dialog(Dialog::None);
    pause(300).await;
    cx.record(
        "a picture edited in another app comes back",
        wrote
            && changed
            && updated.as_ref().is_some_and(|im| {
                (im.width, im.height) == (30, 30) && im.colors == folitecta_core::pictures::Colors::Color
            })
            && path.as_ref().is_some_and(|p| !p.exists()),
        format!("wrote {wrote}, changed {changed}, {updated:?}"),
    );

    // A scan tilted by 4 degrees is straightened.
    let lines = image::RgbaImage::from_fn(240, 180, |_, y| {
        if y % 16 < 3 && (12..168).contains(&y) {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let tilted = folitecta_core::imaging::turn(&lines, 4.0, image::Rgba([255, 255, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = tilted.write_to(&mut png, image::ImageFormat::Png);
    let scan = st
        .with_active(|t| {
            t.doc
                .add_image(0, png.into_inner(), Some([300.0, 120.0, 540.0, 300.0]))
                .ok()?;
            t.doc.page_images(0).ok()?.last().map(|im| im.index)
        })
        .flatten();
    let mut angle = None;
    if let Some(i) = scan {
        st.with_active(|t| t.image_sel = Some((0, i)));
        let rev = st.read_active(|t| t.revision).unwrap_or(0);
        crate::commands::dispatch(st, "imageEditor.deskew");
        for _ in 0..60 {
            pause(150).await;
            if st.read_active(|t| t.revision).unwrap_or(0) != rev {
                break;
            }
        }
        angle = st
            .read_active(|t| t.doc.image_pixels(0, i).ok())
            .flatten()
            .map(|px| folitecta_core::imaging::skew_angle(&px));
    }
    cx.record(
        "a tilted scan is straightened",
        angle.is_some_and(|a| a.abs() < 0.3),
        format!("{angle:?}"),
    );

    // A picture copied elsewhere replaces the selected one.
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = image::RgbaImage::from_pixel(40, 20, image::Rgba([20, 160, 60, 255]))
        .write_to(&mut png, image::ImageFormat::Png);
    let copied = crate::clipboard::set_png(&png.into_inner()).is_ok();
    let replaced = match scan {
        Some(i) => {
            st.set_tool(Tool::EditImages);
            st.with_active(|t| t.image_sel = Some((0, i)));
            crate::commands::dispatch(st, "replaceImageClipboard");
            pause(700).await;
            st.read_active(|t| t.doc.image_pixels(0, i).ok())
                .flatten()
                .map(|px| (px.dimensions(), px.get_pixel(px.width() / 2, px.height() / 2).0))
        }
        None => None,
    };
    cx.record(
        "a picture from the clipboard replaces the selected one",
        copied && replaced.is_some_and(|(size, p)| size == (40, 20) && p[1] > 120 && p[0] < 60),
        format!("copied {copied}, {replaced:?}"),
    );
    st.set_tool(Tool::Hand);
    pause(300).await;
}
