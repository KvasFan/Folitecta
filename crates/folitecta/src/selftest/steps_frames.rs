//! Interaction step: the interface keeps working while no frame is drawn. A
//! minimized window, or one covered by other windows, gets no animation
//! frames on Linux desktops, and the changes of the interface used to wait
//! for one (the whole program stopped). The page holds its frames back for
//! a few seconds; a dialog opened and closed meanwhile must reach the page in
//! the usual time.

use std::time::Instant;

use super::*;

/// Hold the animation frames back, for 5 seconds at most.
const HOLD_FRAMES: &str = r#"
if (window.__foliHeld) return false;
const real = window.requestAnimationFrame.bind(window);
const held = [];
window.__foliHeld = { real, held };
window.requestAnimationFrame = cb => { held.push(cb); return 0; };
window.__foliRelease = () => {
  const h = window.__foliHeld;
  if (!h) return;
  window.__foliHeld = null;
  window.requestAnimationFrame = h.real;
  h.held.forEach(cb => h.real(cb));
};
setTimeout(() => window.__foliRelease(), 5000);
return true;
"#;

async fn flag(js: &str) -> bool {
    eval(js).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let held = flag(HOLD_FRAMES).await;
    let started = Instant::now();
    st.set_dialog(Dialog::About);
    pause(400).await;
    let shown = flag("return !!document.querySelector('.dialog .about');").await;
    st.set_dialog(Dialog::None);
    pause(400).await;
    let closed = flag("return !document.querySelector('.dialog');").await;
    let took = started.elapsed();
    let _ = eval("if (window.__foliRelease) window.__foliRelease(); return true;").await;
    pause(300).await;
    cx.record(
        "changes reach the page while no frame is drawn (a minimized or covered window)",
        held && shown && closed && took < Duration::from_secs(3),
        format!(
            "frames held {held}, dialog shown {shown}, closed {closed}, in {:.1} s",
            took.as_secs_f64()
        ),
    );
}
