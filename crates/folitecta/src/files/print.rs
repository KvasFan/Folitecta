//! Printing through the system dialog: the sheets laid out as pictures in
//! the hidden print view (the print window and its jobs are
//! `ui::features::print` and `commands::print`).

use super::*;

/// Lay the sheets of `job` out in the hidden print view and wait until every
/// picture is decoded. Returns the number of sheets ready.
pub async fn prepare_print(st: AppState, job: crate::state::PrintJob) -> usize {
    st.set_print_job(Some(job));
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
