//! Interaction step: Recompress Pictures — the picture selected on the page
//! chosen by itself, an estimate picture by picture, then applied: that
//! picture scaled down to the e-book resolution, the other picture of the
//! page untouched; Undo takes it back.

use super::*;

/// A photograph-like picture (noise over a gradient), as PNG.
fn photo(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    let img = image::RgbImage::from_fn(w, h, |i, j| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let n = (x & 0x3f) as u8;
        image::Rgb([
            ((i * 200 / w) as u8).saturating_add(n),
            ((j * 200 / h) as u8).saturating_add(n / 2),
            120u8.saturating_add(n),
        ])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::from_images(vec![photo(1600, 2000, 7)]).and_then(|mut d| {
        d.add_image(0, photo(600, 450, 9), Some([40.0, 40.0, 184.0, 148.0]))?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record(
            "recompress pictures",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    let objects = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .page_images(0)
                .map(|l| l.iter().map(|i| (i.object, i.width)).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    };
    let before = objects(st);
    let big_bytes = |st: AppState, id: (u32, u16)| {
        st.read_active(|t| {
            t.doc
                .inner()
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok().map(|s| s.content.len()))
        })
        .flatten()
        .unwrap_or(0)
    };
    let big_before = before.first().map(|b| big_bytes(st, b.0)).unwrap_or(0);
    // The small picture selected, then Recompress Image…
    st.with_active(|t| t.image_sel = Some((0, 1)));
    crate::commands::dispatch(st, "document.recompressImage");
    pause(500).await;
    let scope = text("return document.querySelector('.dialog .recompress-scope')?.value || '';").await;
    let _ = eval(
        "const s = document.querySelector('.dialog .recompress-preset'); s.value = 'ebook'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog .recompress-estimate-btn')?.click();").await;
    let mut summary = String::new();
    for _ in 0..40 {
        pause(150).await;
        summary =
            text("return document.querySelector('.dialog .recompress-summary')?.textContent || '';").await;
        if !summary.is_empty() {
            break;
        }
    }
    let rows =
        text("return String(document.querySelectorAll('.dialog .recompress-list .audit-row').length);").await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
    let after = objects(st);
    let big_after = before.first().map(|b| big_bytes(st, b.0)).unwrap_or(0);
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let undone = objects(st);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "recompress pictures: the selected picture chosen by itself, estimated, scaled down to 150 dpi, the other one untouched, undone",
        scope == "picture"
            && summary.contains('1')
            && rows == "1"
            && after.len() == 2
            && after[1].1 == 300
            && after[0] == before[0]
            && big_after == big_before
            && undone == before,
        format!("scope {scope:?}, summary {summary:?}, rows {rows}, before {before:?}, after {after:?}, undone {undone:?}"),
    );
}
