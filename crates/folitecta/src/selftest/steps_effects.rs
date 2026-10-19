//! Interaction step: the effects of components run. The interface runs the
//! effects of its components (`use_effect`) only when nothing is left to
//! render; a component that writes state it reads while it renders keeps
//! it rendering, and then every effect waits forever. Two components that
//! work through effects are checked in the state other steps leave and with
//! the pane and the dialog that validate signatures: the Split Pages dialog
//! and the picture editor measure their stage with an effect, the files view
//! of a portfolio makes the preview of its selected file with one, and the
//! search pane shows a query started elsewhere with one. Last, no state may
//! be written while a component renders (anywhere in the run so far).

use super::*;
use crate::commands::split::SPLIT;

/// The Split Pages dialog measured its stage (its effect ran).
async fn split_measures(st: AppState) -> bool {
    st.set_dialog(Dialog::None);
    pause(200).await;
    SPLIT.write().stage = (0.0, 0.0);
    crate::commands::split::open(st);
    let mut measured = false;
    for _ in 0..20 {
        pause(150).await;
        if SPLIT.peek().stage.0 > 0.0 {
            measured = true;
            break;
        }
    }
    st.set_dialog(Dialog::None);
    pause(200).await;
    measured
}

/// A picture of 60 × 120 pixels (PNG).
fn picture() -> Vec<u8> {
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = image::RgbaImage::from_pixel(60, 120, image::Rgba([200, 40, 40, 255]))
        .write_to(&mut png, image::ImageFormat::Png);
    png.into_inner()
}

/// Close tab `id` (made by this step).
async fn close(st: AppState, id: Option<u64>) {
    if let Some(id) = id {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}

/// The picture editor measured its stage (its effect ran).
async fn image_editor_measures(st: AppState) -> bool {
    let Ok(mut doc) = folitecta_core::Document::new_blank(1, 300.0, 300.0) else {
        return false;
    };
    if doc.add_image(0, picture(), None).is_err() {
        return false;
    }
    crate::files::open_document_tab(st, doc);
    pause(500).await;
    let tab = st.read_active(|t| t.id);
    crate::commands::pictures::open_editor(st);
    let mut measured = false;
    for _ in 0..20 {
        pause(150).await;
        if crate::commands::pictures::EDITOR.peek().stage.0 > 0.0 {
            measured = true;
            break;
        }
    }
    st.set_dialog(Dialog::None);
    pause(200).await;
    close(st, tab).await;
    measured
}

/// The files view of a portfolio shows the preview of its selected file
/// (made by its effect).
async fn portfolio_previews(st: AppState) -> bool {
    let Ok(doc) = folitecta_core::Document::new_portfolio(vec![("picture.png".into(), picture())], "Cover")
    else {
        return false;
    };
    crate::files::open_document_tab(st, doc);
    pause(500).await;
    let tab = st.read_active(|t| t.id);
    let key = st
        .read_active(|t| {
            t.doc
                .portfolio()
                .and_then(|p| p.items.first().map(|i| i.key.clone()))
        })
        .flatten();
    let mut shown = false;
    if let Some(key) = key {
        st.with_active(|t| t.portfolio.selected = vec![key]);
        for _ in 0..20 {
            pause(150).await;
            shown = eval("return document.querySelectorAll('.portfolio-preview img.pf-thumb').length;")
                .await
                .ok()
                .and_then(|v| v.as_u64())
                .is_some_and(|n| n > 0);
            if shown {
                break;
            }
        }
    }
    close(st, tab).await;
    shown
}

/// The search pane shows a query given elsewhere (its effect ran).
async fn search_follows(st: AppState, query: &str) -> bool {
    st.with_active(|t| t.search.query = query.to_string());
    for _ in 0..20 {
        pause(150).await;
        let shown = eval("return document.querySelector('.search-box input')?.value || '';")
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        if shown == query {
            return true;
        }
    }
    false
}

/// A signed document in a tab of its own (its tab).
async fn signed_document(st: AppState) -> Option<u64> {
    let id = folitecta_core::digsig::DigitalId::create_self_signed(
        &folitecta_core::digsig::IdentityInfo {
            name: "Effects Signer".into(),
            ..Default::default()
        },
        folitecta_core::digsig::KeyKind::EcP256,
        1,
    )
    .ok()?;
    let doc = folitecta_core::Document::new_blank(1, 300.0, 300.0).ok()?;
    let signed = doc.sign_to_file(&id, &Default::default(), None).ok()?;
    let doc = signed.into_document(None).ok()?;
    crate::files::open_document_tab(st, doc);
    pause(500).await;
    st.read_active(|t| t.id)
}

fn show_pane(st: AppState, pane: &str) {
    let mut ui = st.ui_mut();
    ui.show_left_pane = true;
    ui.left_pane = pane.into();
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    if !st.has_doc() {
        crate::files::new_blank(st, 1, 595.0, 842.0);
        pause(500).await;
    }
    let pane = st.ui.peek().left_pane.clone();
    let shown = st.ui.peek().show_left_pane;
    // As the steps before left the window.
    let left_as_is = split_measures(st).await;
    // With the thumbnails, the signatures pane, and the search pane under
    // the details of a signature.
    // The Split Pages dialog, the picture editor and the preview of a
    // portfolio, with the thumbnails and with the signatures pane.
    let mut by_pane = Vec::new();
    for pane in ["thumbnails", "signatures"] {
        show_pane(st, pane);
        pause(500).await;
        let split = split_measures(st).await;
        let editor = image_editor_measures(st).await;
        let preview = portfolio_previews(st).await;
        by_pane.push((pane, split, editor, preview));
    }
    let panes_ok = by_pane.iter().all(|(_, a, b, c)| *a && *b && *c);
    show_pane(st, "search");
    pause(400).await;
    let search = search_follows(st, "effects-one").await;
    // A signed document, and the details of its signature open.
    let signed_tab = signed_document(st).await;
    let field = st
        .read_active(|t| t.doc.signatures())
        .unwrap_or_default()
        .into_iter()
        .find(|s| s.signed)
        .map(|s| s.field);
    let details = match field {
        Some(f) => {
            st.set_dialog(Dialog::SignatureDetails(f));
            pause(600).await;
            let ok = search_follows(st, "effects-two").await;
            st.set_dialog(Dialog::None);
            ok
        }
        None => false,
    };
    st.with_active(|t| t.search.query.clear());
    close(st, signed_tab).await;
    {
        let mut ui = st.ui_mut();
        ui.left_pane = pane;
        ui.show_left_pane = shown;
    }
    pause(300).await;
    cx.record(
        "the effects of components run (Split Pages, picture editor, portfolio preview, search pane): in the state the steps before left, with the thumbnails, the signatures pane and the details of a signature",
        left_as_is && panes_ok && search && details,
        format!(
            "as left {left_as_is}; (pane, split, editor, portfolio preview) {by_pane:?}; search pane {search}, under the details of a signature {details}"
        ),
    );
    let writes = crate::state::render_writes();
    cx.record(
        "no state is written while a component renders",
        writes.is_empty(),
        writes.join(", "),
    );
}
