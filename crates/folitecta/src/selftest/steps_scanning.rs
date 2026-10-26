//! Interaction step: scanning with the self-test's scanner (nothing is
//! started) — the scanner and what it offers listed, a preset saved; three
//! fronts from the feeder, then their backs as a second stack turned over,
//! made into one document of six pages in the order of the sheets; one
//! page from the flatbed without backs.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn wait_for(js: &str) -> bool {
    for _ in 0..40 {
        if eval(js).await.ok().and_then(|v| v.as_bool()).unwrap_or(false) {
            return true;
        }
        pause(150).await;
    }
    false
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    crate::scanner::set_stand_in(true);
    let tabs = st.docs.read().len();
    crate::commands::dispatch(st, "newDocScanner");
    let listed = wait_for("return !!document.querySelector('.dialog .scan-source');").await;
    let device = text("const s = document.querySelector('.dialog .scan-device'); return s ? s.options[s.selectedIndex].text : '';").await;
    // The feeder at 75 dpi, saved as a preset.
    let _ = eval(
        "const s = document.querySelector('.dialog .scan-source'); s.value = 'ADF'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval(
        "const s = document.querySelector('.dialog .scan-dpi'); s.value = '75'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval(
        "const i = document.querySelector('.dialog .scan-preset-name'); i.value = 'Feeder 75'; i.dispatchEvent(new Event('input', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval("[...document.querySelectorAll('.dialog .scan-preset-save button')][0]?.click();").await;
    pause(300).await;
    let preset = crate::commands::scanning::presets(st);
    let saved = preset
        .iter()
        .any(|(n, s)| n == "Feeder 75" && s.source == "ADF" && s.dpi == 75);
    // Fronts, then the backs.
    let backs = eval("const c = document.querySelector('.dialog .scan-backs'); if (c && !c.checked) c.click(); return true;").await.is_ok();
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let turn = wait_for("return !!document.querySelector('.dialog .scan-turn');").await;
    let message = text("return document.querySelector('.dialog .scan-turn span')?.textContent || '';").await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut pages = Vec::new();
    for _ in 0..40 {
        pause(150).await;
        if st.docs.read().len() > tabs {
            pages = st
                .read_active(|t| {
                    t.doc
                        .page_sizes()
                        .iter()
                        .map(|s| (s.width.round(), s.height.round()))
                        .collect()
                })
                .unwrap_or_default();
            break;
        }
    }
    let duplex_tab = st.read_active(|t| t.id).unwrap_or(0);
    // One page from the flatbed, no backs.
    crate::commands::dispatch(st, "newDocScanner");
    let _ = wait_for("return !!document.querySelector('.dialog .scan-source');").await;
    let _ = eval(
        "const s = document.querySelector('.dialog .scan-source'); s.value = 'Flatbed'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ =
        eval("const c = document.querySelector('.dialog .scan-backs'); if (c && c.checked) c.click();").await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut single = 0;
    for _ in 0..40 {
        pause(150).await;
        if st.docs.read().len() > tabs + 1 {
            single = st.read_active(|t| t.doc.page_count()).unwrap_or(0);
            break;
        }
    }
    let single_tab = st.read_active(|t| t.id).unwrap_or(0);
    for id in [single_tab, duplex_tab] {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(200).await;
    }
    crate::scanner::set_stand_in(false);
    cx.record(
        "scanning: the scanner listed, a preset saved, three fronts and their backs made into six pages of Letter paper, one page from the flatbed",
        listed
            && device == "Folitecta test scanner"
            && saved
            && backs
            && turn
            && message.contains('3')
            && pages.len() == 6
            && pages.iter().all(|p| *p == (612.0, 792.0))
            && single == 1,
        format!(
            "listed {listed}, device {device:?}, saved {saved}, turn {turn} ({message:?}), pages {pages:?}, single {single}"
        ),
    );
}
