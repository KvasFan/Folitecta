//! Interaction step: compare_pictures — two documents with the same text
//! whose picture was replaced: the comparison lists a picture difference,
//! named as one, and marks it on the page.

use super::*;

fn png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(24, 16, image::Rgb([r, g, b]));
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let make = |picture: Vec<u8>| {
        folitecta_core::Document::from_text("The same words on both pages.", Default::default()).and_then(
            |mut d| {
                d.add_image(0, picture, Some([100.0, 300.0, 220.0, 380.0]))?;
                d.clear_history();
                Ok(d)
            },
        )
    };
    let (Ok(a), Ok(b)) = (make(png(200, 0, 0)), make(png(0, 0, 200))) else {
        cx.record(
            "compare pictures",
            false,
            "the documents could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, a);
    let old = st.read_active(|t| t.id).unwrap_or(0);
    crate::files::open_document_tab(st, b);
    let new = st.read_active(|t| t.id).unwrap_or(0);
    crate::commands::compare::run(st, old, new, false);
    for _ in 0..40 {
        pause(150).await;
        if st.comparison.read().is_some() {
            break;
        }
    }
    pause(600).await;
    let kinds: Vec<(bool, &'static str)> = st
        .comparison
        .read()
        .as_ref()
        .map(|c| {
            c.result
                .differences
                .iter()
                .map(|d| (d.picture, d.kind.name()))
                .collect()
        })
        .unwrap_or_default();
    let listed = eval("return document.querySelectorAll('.cmp-picture').length;")
        .await
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    let marks = eval("return document.querySelectorAll('.cmp').length;")
        .await
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    *st.comparison.write_unchecked() = None;
    st.docs_mut().retain(|t| t.id != old && t.id != new);
    st.set_active(0);
    pause(400).await;
    cx.record(
        "compare pictures: a replaced picture is one difference, listed as a picture and marked on the page",
        kinds == [(true, "changed")] && listed == 1 && marks > 0,
        format!("differences {kinds:?}, listed {listed}, marks {marks}"),
    );
}
