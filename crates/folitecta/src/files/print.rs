//! Printing: the pages drawn for the printed layout.

use super::*;

/// Lay the pages out in the print view, wait until they are rendered and open
/// the system print dialog.
pub fn start_print(
    st: AppState,
    pages: Vec<crate::state::PrintPage>,
    dpi: f32,
    annotations: bool,
    fit: bool,
) {
    if pages.is_empty() {
        return;
    }
    spawn(async move {
        prepare_print(st, pages, dpi, annotations, fit).await;
        // The system dialog is modal: the page waits until it is closed.
        dioxus::desktop::window().print();
    });
}

/// Lay the pages out in the hidden print view and wait until every page image
/// is decoded. Returns the number of pages ready.
pub async fn prepare_print(
    st: AppState,
    pages: Vec<crate::state::PrintPage>,
    dpi: f32,
    annotations: bool,
    fit: bool,
) -> usize {
    let id = st.alloc_id();
    let scale_milli = ((dpi / 72.0) * 1000.0).round().clamp(500.0, 6000.0) as u32;
    st.set_print_job(Some(crate::state::PrintJob {
        pages,
        scale_milli,
        annotations,
        fit,
        id,
    }));
    st.set_busy(true);
    let js = r#"
for (let i = 0; i < 200 && !document.querySelector('.print-view'); i++) { await new Promise(r => setTimeout(r, 30)); }
const imgs = [...document.querySelectorAll('.print-view img')];
await Promise.all(imgs.map(img => img.decode ? img.decode().catch(() => {}) : Promise.resolve()));
return imgs.filter(img => img.naturalWidth > 0).length;
"#;
    let ready = document::eval(js)
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    st.set_busy(false);
    ready
}
