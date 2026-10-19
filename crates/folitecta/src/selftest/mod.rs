//! Interface self-test (`--ui-selftest <dir>`): resizes the window through a set
//! of sizes and UI states, audits the layout for overlapping or clipped
//! elements and stores a report plus a screenshot of every state.

use std::path::PathBuf;
use std::time::Duration;

use dioxus::desktop::LogicalSize;
use dioxus::prelude::*;

use crate::state::{AppState, Dialog, DocTab, Pane, TextSel, Theme, Tool};
use progress::{begin, eval, finish, step_limit, take_timeouts, wait_limit};

/// Self-test configuration provided through the launch context.
#[derive(Clone, Debug)]
pub struct SelfTestConfig {
    pub dir: PathBuf,
}

/// Layout audit run inside the page. Returns a JSON object with the problems found.
const AUDIT_JS: &str = r#"
const vw = window.innerWidth, vh = window.innerHeight;
const problems = [];
const visible = el => {
  const s = getComputedStyle(el);
  if (s.display === 'none' || s.visibility === 'hidden' || parseFloat(s.opacity) === 0) return false;
  const r = el.getBoundingClientRect();
  return r.width > 0.5 && r.height > 0.5;
};
const desc = el => {
  const cls = (el.className && el.className.baseVal === undefined) ? String(el.className).trim().split(/\s+/).slice(0, 3).join('.') : '';
  const text = (el.innerText || el.getAttribute('title') || '').trim().replace(/\s+/g, ' ').slice(0, 40);
  return el.tagName.toLowerCase() + (cls ? '.' + cls : '') + (text ? ' "' + text + '"' : '');
};
const clippedByScroller = el => {
  for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
    const s = getComputedStyle(p);
    if (/(auto|scroll)/.test(s.overflowX + s.overflowY)) return true;
  }
  return false;
};
// 1. Nothing outside the window (except content of scrolling containers).
const roots = ['.menubar', '.ribbon-tabs', '.ribbon-body', '.statusbar', '.pane-strip', '.pane-header', '.pane-toolbar', '.doc-tabs', '.dialog', '.rbtn-menu', '.menu-dropdown', '.submenu', '.sel-toolbar', '.search-box', '.search-nav', '.right-pane', '.toasts'];
for (const sel of roots) {
  for (const root of document.querySelectorAll(sel)) {
    if (!visible(root)) continue;
    for (const el of [root, ...root.querySelectorAll('*')]) {
      if (!visible(el) || clippedByScroller(el) && el !== root) continue;
      const r = el.getBoundingClientRect();
      if (r.right > vw + 1 || r.bottom > vh + 1 || r.left < -1 || r.top < -1) {
        problems.push({ kind: 'offscreen', el: desc(el), rect: [r.left, r.top, r.right, r.bottom].map(Math.round) });
      }
    }
  }
}
// 2. Siblings in rows must not overlap.
const rows = ['.menubar', '.ribbon-tabs', '.ribbon-right', '.qat', '.qat-items', '.ribbon-body', '.ribbon-group-items', '.statusbar', '.statusbar .seg', '.pane-toolbar', '.search-nav', '.search-opts', '.dialog-foot', '.field-grid', '.radio-row', '.label-row', '.doc-tabs', '.pane-header', '.welcome .actions', '.check-grid', '.stamp-grid', '.row', '.sel-toolbar', '.list-item .title', '.list-item .meta'];
for (const sel of rows) {
  for (const row of document.querySelectorAll(sel)) {
    if (!visible(row)) continue;
    const kids = [...row.children].filter(visible).filter(k => getComputedStyle(k).position !== 'absolute' && getComputedStyle(k).position !== 'fixed');
    for (let i = 0; i < kids.length; i++) {
      const a = kids[i].getBoundingClientRect();
      for (let j = i + 1; j < kids.length; j++) {
        const b = kids[j].getBoundingClientRect();
        const ox = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const oy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ox > 1.5 && oy > 1.5) problems.push({ kind: 'overlap', row: sel, a: desc(kids[i]), b: desc(kids[j]), area: Math.round(ox * oy) });
      }
    }
  }
}
// 3. Text that does not fit its box without an ellipsis.
for (const el of document.querySelectorAll('.menu-title, .ribbon-tab, .ribbon-group-title, .dialog-head, .field label, .check span, .radio-row label, .tbtn, .choice b, .kv .k, .pane-title, .statusbar span')) {
  if (!visible(el)) continue;
  const s = getComputedStyle(el);
  if (el.scrollWidth > el.clientWidth + 2 && s.textOverflow !== 'ellipsis' && s.overflow !== 'visible') {
    problems.push({ kind: 'clipped-text', el: desc(el), need: el.scrollWidth, have: el.clientWidth });
  }
}
// 3b. Buttons and inputs squeezed below their content height.
for (const el of document.querySelectorAll('.tbtn, .rbtn, .ibtn, .menu-title, .ribbon-tab, .chip, .style-row input, .field input, .field select')) {
  if (!visible(el)) continue;
  const s = getComputedStyle(el);
  if (s.overflowY !== 'visible' && el.scrollHeight > el.clientHeight + 3) {
    problems.push({ kind: 'squeezed', el: desc(el), need: el.scrollHeight, have: el.clientHeight });
  }
}
// 4. The ribbon and the whole app must not scroll horizontally.
const rb = document.querySelector('.ribbon-body');
if (rb && rb.scrollWidth > rb.clientWidth + 2) problems.push({ kind: 'ribbon-overflow', need: rb.scrollWidth, have: rb.clientWidth });
const app = document.querySelector('.app');
if (app && (app.scrollWidth > vw + 1 || app.scrollHeight > vh + 1)) problems.push({ kind: 'app-overflow', w: app.scrollWidth, h: app.scrollHeight });
// 5. Text and icons of the interface stand out from what is behind them
// (contrast ratio of at least 3, the backgrounds of the ancestors composed).
const rgba = c => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1]; };
const lum = c => { const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
const behind = el => {
  const chain = [];
  for (let p = el; p; p = p.parentElement) chain.push(p);
  let base = [255, 255, 255];
  for (const p of chain.reverse()) {
    const b = rgba(getComputedStyle(p).backgroundColor);
    if (b && b[3] > 0) base = base.map((v, i) => b[i] * b[3] + v * (1 - b[3]));
  }
  return base;
};
const faded = el => { for (let p = el; p; p = p.parentElement) { if (parseFloat(getComputedStyle(p).opacity) < 0.9) return true; } return false; };
const chrome = '.menubar, .ribbon, .pane-strip, .pane-header, .pane-toolbar, .statusbar, .menu-dropdown, .submenu, .rbtn-menu, .dialog-head, .dialog-foot, .doc-tabs';
for (const root of document.querySelectorAll(chrome)) {
  if (!visible(root)) continue;
  for (const el of [root, ...root.querySelectorAll('*')]) {
    const isIcon = el.tagName.toLowerCase() === 'svg' && el.classList.contains('ic');
    const hasText = [...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim());
    if (!(isIcon || hasText) || !visible(el) || faded(el) || el.disabled) continue;
    const fg = rgba(getComputedStyle(el).color);
    if (!fg) continue;
    const bg = behind(el);
    const a = lum(fg), b = lum(bg);
    const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    if (ratio < 3) problems.push({ kind: 'low-contrast', el: desc(el), ratio: Math.round(ratio * 100) / 100 });
  }
}
return JSON.stringify({ vw, vh, sw: screen.availWidth, sh: screen.availHeight, problems });
"#;

async fn pause(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// Give the window an inner size. Only a maximized window is unmaximized
/// first: on Wayland the request goes to the compositor even for a window
/// that is not maximized, and its answer (the current size) arrived after the
/// new size and put the old one back, so most sizes were never applied.
fn set_size(w: f64, h: f64) {
    let window = dioxus::desktop::window();
    if window.is_maximized() {
        window.set_maximized(false);
    }
    window.set_inner_size(LogicalSize::new(w, h));
    progress::asked_size(w, h);
}

/// A sample document with text, marks, comments and bookmarks.
fn sample_document() -> folitecta_core::Result<folitecta_core::Document> {
    let para = "Folitecta renders, annotates and organizes PDF documents. The quick brown fox jumps over the lazy dog. \
Pack my box with five dozen liquor jugs. Visit https://github.com/KvasFan/Folitecta for the source code. ";
    let mut text = String::new();
    for i in 1..=60 {
        text.push_str(&format!("Section {i}\n{}\n\n", para.repeat(3)));
    }
    let mut doc = folitecta_core::Document::from_text(&text, folitecta_core::TextLayout::default())?;
    let n = doc.page_count();
    let all: Vec<usize> = (0..n).collect();
    doc.add_text_watermark(&all, &folitecta_core::TextMarkOptions::default())?;
    doc.add_page_numbers(
        &all,
        "Page {page} of {pages}",
        1,
        folitecta_core::MarkPosition::BottomCenter,
        9.0,
        24.0,
        [0.3, 0.3, 0.3],
    )?;
    doc.add_note(
        0,
        520.0,
        780.0,
        "Review this paragraph",
        "Tester",
        [1.0, 0.85, 0.2],
    )?;
    doc.add_shape(
        0,
        folitecta_core::Shape::Square,
        [60.0, 600.0, 260.0, 680.0],
        [0.86, 0.15, 0.15],
        None,
        2.0,
        "Tester",
        "",
    )?;
    doc.add_stamp(
        1,
        [320.0, 700.0, 520.0, 750.0],
        "APPROVED",
        [0.1, 0.5, 0.2],
        "Tester",
    )?;
    for p in 0..n.min(5) {
        doc.add_bookmark(&format!("Chapter {}", p + 1), p, &[])?;
    }
    doc.clear_history();
    Ok(doc)
}

/// Audit the layout and capture the window; returns the number of problems.
/// A state whose audit could not run (no answer, a script error) counts as one
/// problem, and so do the scripts left without an answer before it.
async fn check(st: AppState, dir: &std::path::Path, name: &str, log: &mut Vec<serde_json::Value>) -> usize {
    let started = begin("state", name);
    pause(700).await;
    let drawn = progress::window_drawn().await;
    let audit = match eval(AUDIT_JS).await {
        Ok(v) => v
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or(v),
        Err(e) => serde_json::json!({ "error": e }),
    };
    let mut count = audit
        .get("problems")
        .and_then(|p| p.as_array())
        .map(|a| a.len())
        .unwrap_or(1);
    let timeouts = take_timeouts();
    count += timeouts.len();
    let mut entry = serde_json::json!({ "state": name, "audit": audit, "drawn": drawn });
    // An audit at another size than the one asked checks the wrong layout. A
    // size larger than the screen may be cut by the system (noted only).
    let got = (audit["vw"].as_f64(), audit["vh"].as_f64());
    if let (Some((w, h)), (Some(vw), Some(vh))) = (progress::asked(), got) {
        if (vw - w).abs() > 2.0 || (vh - h).abs() > 2.0 {
            let screen = (audit["sw"].as_f64(), audit["sh"].as_f64());
            let fits = match screen {
                (Some(sw), Some(sh)) => w <= sw && h <= sh,
                _ => true,
            };
            if fits {
                count += 1;
            }
            entry["size"] = serde_json::json!({ "asked": [w, h], "got": [vw, vh], "screen": [screen.0, screen.1], "counted": fits });
        }
    }
    if cfg!(target_os = "windows") {
        let shot = dir.join(format!("{name}.png"));
        let capture = capture_window(&shot);
        entry["screenshot"] = shot.display().to_string().into();
        entry["capture"] = capture.err().into();
    }
    if !timeouts.is_empty() {
        entry["timeouts"] = timeouts.into();
    }
    progress::report(&entry, count);
    log.push(entry);
    finish("state", name, &format!("{count} problem(s)"), started);
    let _ = st;
    count
}

#[cfg(target_os = "windows")]
fn capture_window(path: &std::path::Path) -> Result<(), String> {
    capture_window_from(path, false)
}

/// Capture by copying the window area from the screen (works while the page
/// is blocked by a modal browser dialog, which PrintWindow waits for).
#[cfg(target_os = "windows")]
fn capture_window_from(path: &std::path::Path, from_screen: bool) -> Result<(), String> {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    let hwnd = dioxus::desktop::window().window.hwnd();
    let (w, h, mut rgba) = win_capture::capture(hwnd, from_screen)?;
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("bad image buffer")?;
    img.save(path).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn capture_window(_path: &std::path::Path) -> Result<(), String> {
    Err("screenshots are only captured on Windows".into())
}

#[cfg(target_os = "windows")]
mod win_capture {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    /// Capture the client area (BGRA rows, top-down). Uses PrintWindow with full
    /// content rendering and falls back to copying from the screen.
    pub fn capture(hwnd_raw: isize, screen_only: bool) -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
            let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
            if w <= 0 || h <= 0 {
                return Err("empty window".into());
            }
            let try_grab = |from_screen: bool| -> Result<Vec<u8>, String> {
                let src_dc = if from_screen {
                    GetDC(None)
                } else {
                    GetDC(Some(hwnd))
                };
                let mem_dc = CreateCompatibleDC(Some(src_dc));
                let bmp = CreateCompatibleBitmap(src_dc, w, h);
                let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
                if from_screen {
                    let mut origin = POINT { x: 0, y: 0 };
                    let _ = ClientToScreen(hwnd, &mut origin);
                    let _ = BitBlt(mem_dc, 0, 0, w, h, Some(src_dc), origin.x, origin.y, SRCCOPY);
                } else {
                    // PW_CLIENTONLY | PW_RENDERFULLCONTENT
                    let _ = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(1 | 2));
                }
                let mut info = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut buf = vec![0u8; (w * h * 4) as usize];
                let lines = GetDIBits(
                    mem_dc,
                    bmp,
                    0,
                    h as u32,
                    Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                    &mut info,
                    DIB_RGB_COLORS,
                );
                SelectObject(mem_dc, old);
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                let _ = DeleteDC(mem_dc);
                if from_screen {
                    ReleaseDC(None, src_dc);
                } else {
                    ReleaseDC(Some(hwnd), src_dc);
                }
                if lines == 0 {
                    return Err("GetDIBits failed".into());
                }
                Ok(buf)
            };
            if screen_only {
                return Ok((w as u32, h as u32, try_grab(true)?));
            }
            let buf = try_grab(false)?;
            // A black frame means the compositor content was not captured.
            let dark = buf
                .as_chunks::<4>()
                .0
                .iter()
                .step_by(97)
                .filter(|p| p[0] < 8 && p[1] < 8 && p[2] < 8)
                .count();
            let total = buf.len() / 4 / 97;
            let buf = if dark * 10 > total * 9 {
                try_grab(true)?
            } else {
                buf
            };
            Ok((w as u32, h as u32, buf))
        }
    }
}

/// Simulate pointer input on page `page` of the canvas. `points` are in page
/// display points; the first is the press, the last the release.
async fn pointer_path(page: usize, zoom: f32, points: &[(f32, f32)], target_first_at_point: bool) -> String {
    let pts: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("[{},{}]", x * zoom, y * zoom))
        .collect();
    let js = format!(
        r#"
const el = document.querySelector('#pg-{page} .page-inner');
if (!el) return 'missing page';
const r = el.getBoundingClientRect();
const pts = [{pts}];
const at = (p) => ({{ clientX: r.left + p[0], clientY: r.top + p[1] }});
const fire = (target, type, p, buttons) => target.dispatchEvent(new MouseEvent(type, Object.assign({{ bubbles: true, cancelable: true, button: 0, buttons }}, at(p))));
let first = el;
if ({hit}) {{ const c = at(pts[0]); first = document.elementFromPoint(c.clientX, c.clientY) || el; }}
fire(first, 'mousedown', pts[0], 1);
for (let i = 1; i < pts.length; i++) fire(el, 'mousemove', pts[i], 1);
fire(el, 'mouseup', pts[pts.length - 1], 0);
return first.className || first.tagName;
"#,
        pts = pts.join(","),
        hit = if target_first_at_point { "true" } else { "false" }
    );
    match eval(&js).await {
        Ok(v) => v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()),
        Err(e) => format!("eval error: {e}"),
    }
}

/// Interpolate a straight drag with `steps` intermediate moves.
fn line(from: (f32, f32), to: (f32, f32), steps: usize) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        })
        .collect()
}

fn annots_on(st: AppState, page: usize) -> Vec<folitecta_core::Annotation> {
    st.read_active(|t| t.doc.annotations(page).unwrap_or_default())
        .unwrap_or_default()
}

mod progress;
/// Exercise the canvas tools with simulated input; returns the number of failures.
mod steps_arrange;
mod steps_barcodes;
mod steps_calc;
mod steps_email;
mod steps_features;
mod steps_field_items;
mod steps_files;
mod steps_frames;
mod steps_layer_drawing;
mod steps_layout;
mod steps_links;
mod steps_note_icon;
mod steps_ocr;
mod steps_panes;
mod steps_pictures;
mod steps_policies;
mod steps_protection;
mod steps_qat;
mod steps_recovery;
mod steps_review;
mod steps_spelling;
mod steps_split;
mod steps_split_view;
mod steps_text;
mod steps_timestamp;
mod steps_tools;
mod steps_unicode;

/// What the interaction steps share: the app, where the screenshots go,
/// the report and the zoom of the pages.
struct Ctx<'a> {
    st: AppState,
    dir: &'a std::path::Path,
    log: &'a mut Vec<serde_json::Value>,
    failures: usize,
    z: f32,
}

impl Ctx<'_> {
    fn record(&mut self, name: &str, pass: bool, detail: String) {
        if !pass {
            self.failures += 1;
        }
        let took = progress::took();
        progress::line(&format!(
            "  {} {name} ({:.1} s)",
            if pass { "pass" } else { "FAIL" },
            took.as_secs_f64()
        ));
        let entry = serde_json::json!({ "interaction": name, "pass": pass, "detail": detail });
        progress::report(&entry, usize::from(!pass));
        self.log.push(entry);
    }
}

/// Run the interaction step named `name` with a time limit: its scripts left
/// without an answer and a step that does not end in time are failures, never
/// a hang.
async fn run_step(cx: &mut Ctx<'_>, name: &str) {
    let started = begin("step", name);
    progress::window_drawn().await;
    let before = cx.failures;
    let ended = {
        let fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>> = match name {
            "tools" => Box::pin(steps_tools::run(cx)),
            "review" => Box::pin(steps_review::run(cx)),
            "panes" => Box::pin(steps_panes::run(cx)),
            "features" => Box::pin(steps_features::run(cx)),
            "files" => Box::pin(steps_files::run(cx)),
            "pictures" => Box::pin(steps_pictures::run(cx)),
            "split" => Box::pin(steps_split::run(cx)),
            "text" => Box::pin(steps_text::run(cx)),
            "layout" => Box::pin(steps_layout::run(cx)),
            "recovery" => Box::pin(steps_recovery::run(cx)),
            "barcodes" => Box::pin(steps_barcodes::run(cx)),
            "calc" => Box::pin(steps_calc::run(cx)),
            "split_view" => Box::pin(steps_split_view::run(cx)),
            "links" => Box::pin(steps_links::run(cx)),
            "note_icon" => Box::pin(steps_note_icon::run(cx)),
            "arrange" => Box::pin(steps_arrange::run(cx)),
            "spelling" => Box::pin(steps_spelling::run(cx)),
            "ocr" => Box::pin(steps_ocr::run(cx)),
            "field_items" => Box::pin(steps_field_items::run(cx)),
            "policies" => Box::pin(steps_policies::run(cx)),
            "qat" => Box::pin(steps_qat::run(cx)),
            "email" => Box::pin(steps_email::run(cx)),
            "timestamp" => Box::pin(steps_timestamp::run(cx)),
            "protection" => Box::pin(steps_protection::run(cx)),
            "frames" => Box::pin(steps_frames::run(cx)),
            "unicode" => Box::pin(steps_unicode::run(cx)),
            "layer_drawing" => Box::pin(steps_layer_drawing::run(cx)),
            _ => Box::pin(async { cx.record("unknown step", false, name.to_string()) }),
        };
        tokio::time::timeout(step_limit(), fut).await.is_ok()
    };
    let timeouts = take_timeouts();
    if !timeouts.is_empty() {
        cx.record(
            &format!("step {name}: the page answers every script"),
            false,
            timeouts.join(" | "),
        );
    }
    if !ended {
        cx.record(
            &format!("step {name}: ends within the time limit"),
            false,
            format!("stopped after {:.0} s", step_limit().as_secs_f64()),
        );
    }
    let failed = cx.failures - before;
    finish("step", name, &format!("{failed} failure(s)"), started);
}

/// The interaction steps in the order of a full run.
const STEPS: [&str; 27] = [
    "tools",
    "review",
    "panes",
    "features",
    "files",
    "pictures",
    "split",
    "text",
    "layout",
    "recovery",
    "barcodes",
    "calc",
    "split_view",
    "links",
    "note_icon",
    "arrange",
    "spelling",
    "ocr",
    "field_items",
    "policies",
    "qat",
    "email",
    "timestamp",
    "protection",
    "frames",
    "unicode",
    "layer_drawing",
];

async fn interactions(st: AppState, dir: &std::path::Path, log: &mut Vec<serde_json::Value>) -> usize {
    set_size(1280.0, 800.0);
    st.ui_mut().left_pane = "thumbnails".into();
    st.with_active(|t| {
        t.zoom = crate::state::Zoom::Fixed(1.0);
        t.layout = crate::state::Layout::Continuous;
        t.view_rotation = 0;
        t.search.hits.clear();
    });
    st.go_to_page(0);
    pause(1200).await;
    let z = 1.0;
    let mut cx = Ctx {
        st,
        dir,
        log,
        failures: 0,
        z,
    };
    for name in STEPS {
        run_step(&mut cx, name).await;
    }
    let failures = cx.failures;

    // 58. Undo everything added above.
    st.set_tool(Tool::Hand);
    for _ in 0..100 {
        crate::commands::dispatch(st, "undo");
    }
    pause(500).await;
    st.with_active(|t| t.zoom = crate::state::Zoom::FitWidth);
    failures
}

/// Run the whole self-test, write `report.json` and exit.
pub async fn run(st: AppState, cfg: SelfTestConfig) {
    let dir = cfg.dir.clone();
    let _ = std::fs::create_dir_all(&dir);
    progress::open(&dir);
    let mut log: Vec<serde_json::Value> = Vec::new();
    let mut total = 0;
    // FOLITECTA_SELFTEST_ONLY=print runs only the print step,
    // =interactions every interaction step, =step:<name> one of them.
    let only = std::env::var("FOLITECTA_SELFTEST_ONLY").ok();
    progress::line(&format!(
        "self-test of Folitecta {} on {}, only {only:?}, script limit {:.0} s, step limit {:.0} s",
        folitecta_core::VERSION,
        std::env::consts::OS,
        wait_limit().as_secs_f64(),
        step_limit().as_secs_f64()
    ));
    // A neutral author name: screenshots never show the account name.
    st.ui_mut().author = "Reviewer".into();
    let full = only.is_none();
    // Welcome screen (no document) at the smallest and a large size.
    pause(1500).await;
    for (w, h) in if full {
        vec![(960.0, 620.0), (1920.0, 1080.0)]
    } else {
        Vec::new()
    } {
        set_size(w, h);
        pause(900).await;
        total += check(st, &dir, &format!("{}x{}-welcome", w as u32, h as u32), &mut log).await;
    }
    match sample_document() {
        Ok(doc) => {
            let id = st.alloc_id();
            st.docs_mut().push(DocTab::new(id, doc));
            let n = st.docs.read().len();
            st.set_active(n - 1);
        }
        Err(e) => eprintln!("sample document: {e}"),
    }
    pause(2500).await;
    if full {
        total += full_suite(st, &dir, &mut log).await;
    } else if only.as_deref() == Some("interactions") {
        total += interactions(st, &dir, &mut log).await;
    } else if let Some(step) = only.as_deref().and_then(|o| o.strip_prefix("step:")) {
        // FOLITECTA_SELFTEST_ONLY=step:<name> runs one interaction step (for debugging).
        set_size(1280.0, 800.0);
        st.with_active(|t| t.zoom = crate::state::Zoom::Fixed(1.0));
        pause(800).await;
        let mut cx = Ctx {
            st,
            dir: &dir,
            log: &mut log,
            failures: 0,
            z: 1.0,
        };
        run_step(&mut cx, step).await;
        total += cx.failures;
    }
    total += print_layout(st, &dir, &mut log).await;
    // What was checked while the window was not drawn may be wrong.
    let hidden = progress::not_drawn();
    let pass = hidden.is_empty();
    let entry = serde_json::json!({
        "interaction": "the window is drawn during the whole run (not minimized or covered)",
        "pass": pass,
        "detail": format!(
            "{} checks or steps without frames: {}",
            hidden.len(),
            hidden.iter().take(20).cloned().collect::<Vec<_>>().join(", ")
        ),
    });
    progress::report(&entry, usize::from(!pass));
    log.push(entry);
    total += usize::from(!pass);
    let report = serde_json::json!({ "total_problems": total, "complete": true, "states": log });
    let _ = std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap_or_default(),
    );
    progress::done(total);
    println!(
        "ui self-test finished: {total} problem(s); report in {}",
        dir.display()
    );
    // Leaving on purpose: no recovery copies for the next run.
    crate::recovery::forget_session();
    std::process::exit(if total == 0 { 0 } else { 3 });
}

/// The printed layout of the first pages, emulated on screen (the system
/// dialog is modal): every page image decoded and laid out. Returns the
/// number of problems.
async fn print_layout(st: AppState, dir: &std::path::Path, log: &mut Vec<serde_json::Value>) -> usize {
    let started = begin("step", "print");
    set_size(1280.0, 800.0);
    pause(800).await;
    let pages: Vec<crate::state::PrintPage> = st
        .read_active(|t| {
            (0..3.min(t.doc.page_count()))
                .map(|p| (t.id, p, t.revision))
                .collect()
        })
        .unwrap_or_default();
    let ready = tokio::time::timeout(
        wait_limit() * 2,
        crate::files::prepare_print(st, pages, 100.0, true, true),
    )
    .await
    .ok();
    let _ = eval("document.documentElement.classList.add('emulate-print'); window.scrollTo(0, 0);").await;
    pause(800).await;
    let shown = eval(
        "return [...document.querySelectorAll('.print-view .print-page img')].filter(i => i.complete && i.naturalWidth > 0 && i.getBoundingClientRect().width > 100).length;",
    )
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
    let mut entry = serde_json::json!({ "interaction": "print layout with 3 rendered pages" });
    let mut pass = ready == Some(3) && shown == 3;
    if cfg!(target_os = "windows") {
        let shot = dir.join("print-layout.png");
        let capture = capture_window(&shot);
        pass &= capture.is_ok();
        entry["screenshot"] = shot.display().to_string().into();
        entry["capture"] = capture.err().into();
    }
    let _ = eval("document.documentElement.classList.remove('emulate-print');").await;
    let timeouts = take_timeouts();
    pass &= timeouts.is_empty();
    let detail = match ready {
        Some(n) => format!("{n} pages ready, {shown} shown"),
        None => format!(
            "the pages were not ready within {:.0} s",
            (wait_limit() * 2).as_secs_f64()
        ),
    };
    entry["pass"] = pass.into();
    entry["detail"] = if timeouts.is_empty() {
        detail
    } else {
        format!("{detail}; no answer to: {}", timeouts.join(" | "))
    }
    .into();
    progress::report(&entry, usize::from(!pass));
    log.push(entry);
    finish("step", "print", if pass { "pass" } else { "FAIL" }, started);
    usize::from(!pass)
}

/// The e-mail dialog with its first file prepared (pages, what was left
/// out, the first pages shown) and, with `failed`, a mail program that did
/// not take the files. Returns the number of problems.
async fn mail_dialog(
    st: AppState,
    dir: &std::path::Path,
    tag: &str,
    failed: bool,
    log: &mut Vec<serde_json::Value>,
) -> usize {
    use crate::commands::mail::{self, MAIL, Pages, Phase, Prep, Start};
    mail::start(
        st,
        Start::Documents {
            all: false,
            zip: false,
        },
    );
    mail::edit_row(st, 0, |it| {
        it.pages = Pages::Range;
        it.range = "1".into();
        it.comments = folitecta_core::CopyComments::Flatten;
    });
    for _ in 0..100 {
        pause(200).await;
        let ready = MAIL.peek().as_ref().is_some_and(|d| {
            d.items
                .first()
                .is_some_and(|i| !matches!(i.prep, Prep::Pending | Prep::Running(_)))
        });
        if ready {
            break;
        }
    }
    let mut total = check(st, dir, &format!("960x620-{tag}-dialog-mail"), log).await;
    if failed {
        mail::edit_text(|d| {
            d.phase = Phase::Failed(crate::mail::HandOverError::NoProgram("self-test".into()))
        });
        total += check(st, dir, &format!("960x620-{tag}-dialog-mail-failed"), log).await;
    }
    st.set_dialog(Dialog::None);
    total
}

/// Every layout state and interaction check; returns the number of problems.
async fn full_suite(st: AppState, dir: &std::path::Path, log: &mut Vec<serde_json::Value>) -> usize {
    let dir = dir.to_path_buf();
    let mut total = interactions(st, &dir, log).await;
    let sizes: [(f64, f64); 4] = [(960.0, 620.0), (1280.0, 800.0), (1600.0, 900.0), (1920.0, 1080.0)];
    let tabs = [
        "home", "view", "comment", "protect", "form", "organize", "convert", "share", "review", "acc", "help",
    ];
    for (w, h) in sizes {
        set_size(w, h);
        pause(900).await;
        let tag = format!("{}x{}", w as u32, h as u32);
        for tab in tabs {
            st.set_ribbon_tab(tab.to_string());
            total += check(st, &dir, &format!("{tag}-tab-{tab}"), log).await;
        }
        st.set_ribbon_tab("home".into());
        // Both side panes open.
        st.ui_mut().show_right_pane = true;
        total += check(st, &dir, &format!("{tag}-both-panes"), log).await;
        st.ui_mut().show_right_pane = false;
        // Search results and text selection.
        st.ui_mut().left_pane = "search".into();
        crate::ui::panes::run_search(st, "fox");
        st.set_tool(Tool::Select);
        st.with_active(|t| {
            t.text_sel = Some(TextSel {
                page: t.page,
                anchor: 0,
                head: 60,
            })
        });
        total += check(st, &dir, &format!("{tag}-search-select"), log).await;
        st.with_active(|t| t.text_sel = None);
        st.set_tool(Tool::Hand);
        // Comments pane, rulers and grid.
        st.ui_mut().left_pane = "comments".into();
        st.set_option("view.showRulers", true);
        st.set_option("view.showGrid", true);
        total += check(st, &dir, &format!("{tag}-comments-rulers"), log).await;
        st.set_option("view.showRulers", false);
        st.set_option("view.showGrid", false);
        // Layers pane with a group, nested layers and a view override.
        let added = st.with_active(|t| {
            let g = t
                .doc
                .add_layer_group("Annotations layer group with a long name", &[])
                .ok()?;
            let (a, _) = t.doc.add_layer("Drawings", &g).ok()?;
            t.doc.add_layer("Measurements and dimensions", &[]).ok()?;
            t.selected_layers = vec![g];
            t.refresh();
            t.set_layer_view(std::collections::HashMap::from([(a, false)]));
            Some(())
        });
        st.ui_mut().left_pane = "layers".into();
        total += check(st, &dir, &format!("{tag}-layers-pane"), log).await;
        if added.flatten().is_some() {
            st.with_active(|t| {
                t.layer_view.clear();
                t.selected_layers.clear();
                for _ in 0..3 {
                    let _ = t.doc.undo();
                }
                t.refresh();
            });
        }
        // Accessibility panes on the document tagged for the moment.
        let tagged = st
            .with_active(|t| {
                let ok = t.doc.auto_tag().is_ok();
                t.refresh();
                ok
            })
            .unwrap_or(false);
        if tagged {
            crate::commands::accessibility::run_check(st);
            for pane in [
                "tags",
                "order",
                "content",
                "accessibility-checker",
                "accessibility-report",
            ] {
                st.ui_mut().left_pane = pane.into();
                total += check(st, &dir, &format!("{tag}-pane-{pane}"), log).await;
            }
            st.with_active(|t| {
                t.acc = Default::default();
                t.flash = None;
                let _ = t.doc.undo();
                t.refresh();
            });
        }
        st.ui_mut().left_pane = "thumbnails".into();
        // A menu with a submenu opened on its first entry that has children,
        // and a ribbon overflow dropdown near the right edge.
        st.set_open_menu(Some("help".into()));
        pause(400).await;
        let _ = eval(
            "const it = [...document.querySelectorAll('.menu-dropdown .menu-item')].find(e => e.querySelector('.chev')); if (it) { const r = it.getBoundingClientRect(); it.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, clientX: r.left + 20, clientY: r.top + 8 })); it.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false, clientX: r.left + 20, clientY: r.top + 8 })); it.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left + 20, clientY: r.top + 8 })); }",
        )
        .await;
        total += check(st, &dir, &format!("{tag}-menu-help-submenu"), log).await;
        st.close_menus();
        let _ = eval(
            "const b = [...document.querySelectorAll('.ribbon-body .rbtn.has-menu')].pop(); if (b) { const r = b.getBoundingClientRect(); b.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left + 5, clientY: r.top + 5 })); }",
        )
        .await;
        total += check(st, &dir, &format!("{tag}-ribbon-dropdown"), log).await;
        st.close_menus();
    }
    // Dialogs, languages, themes and the classic interface at the smallest size.
    set_size(960.0, 620.0);
    pause(900).await;
    let dialogs = [
        ("security", Dialog::Security),
        ("labels", Dialog::PageLabels),
        ("stamps", Dialog::StampPicker),
        ("watermark", Dialog::Watermark),
        ("headerfooter", Dialog::HeaderFooter),
        ("prefs", Dialog::Prefs),
        ("paste", Dialog::PasteSpecial),
        ("transitions", Dialog::Transitions),
        ("newblank", Dialog::NewBlank),
        ("matrix", Dialog::Matrix),
        ("summary", Dialog::SummarizeComments),
        ("comment-filter", Dialog::CommentFilter),
        ("comments-options", Dialog::CommentsViewOptions),
        ("bookmark-props", Dialog::BookmarkProps(vec![0])),
        ("bookmark-options", Dialog::BookmarkOptions),
        ("bookmark-io", Dialog::BookmarkImportExport),
        (
            "sign",
            Dialog::Sign {
                certify: 0,
                invisible: false,
                field: None,
            },
        ),
        ("timestamp", Dialog::TimeStamp),
        (
            "password",
            Dialog::Password {
                target: crate::state::PasswordTarget::Replace {
                    path: "signed.pdf".into(),
                    tab: 0,
                },
                retry: true,
            },
        ),
        ("digital-ids", Dialog::DigitalIds),
        ("trusted-certs", Dialog::TrustedCerts),
        ("signature-library", Dialog::SignatureLibrary),
        ("bates", Dialog::Bates),
        ("macro-guide", Dialog::MacroGuide),
        ("launch-app", Dialog::LaunchApp),
        (
            "layer-name",
            Dialog::LayerName {
                group: false,
                rename: None,
            },
        ),
        ("layer-overrides", Dialog::LayerOverrides),
        ("layer-options", Dialog::LayerOptions),
        ("choose-layer", Dialog::ChooseLayer),
        ("move-to-new-layer", Dialog::MoveToNewLayer),
        ("attachment-options", Dialog::AttachmentOptions),
        ("fields-options", Dialog::FieldsOptions),
        ("space-audit", Dialog::SpaceAudit),
        ("recompress", Dialog::Recompress(false)),
        ("insert-text", Dialog::InsertTextPages),
        ("document-actions", Dialog::DocumentActions),
        ("manage-marks", Dialog::ManageMarks("Watermark".into())),
        ("stamp-designer", Dialog::StampDesigner),
        (
            "table-new",
            Dialog::TableNew {
                page: 0,
                rect: [100.0, 100.0, 300.0, 200.0],
            },
        ),
        ("table-editor", Dialog::TableEditor),
        ("field-copies", Dialog::FieldCopies((1, 0))),
        (
            "tag-new",
            Dialog::TagNew {
                parent: None,
                index: None,
            },
        ),
        ("role-map", Dialog::RoleMap),
        ("class-map", Dialog::ClassMap),
        ("alt-text", Dialog::AltText),
        ("acc-options", Dialog::AccOptions("accessibility-checker".into())),
        ("order-options", Dialog::AccOptions("order".into())),
        ("rule-explain", Dialog::RuleExplain("tagged".into())),
        ("doc-language", Dialog::DocLanguage),
        ("calc-settings", Dialog::CalcSettings),
        ("rename-doc", Dialog::RenameDoc("report-final-version.pdf".into())),
        ("meta-templates", Dialog::MetaTemplates),
        ("meta-template-save", Dialog::MetaTemplateSave),
        ("js-console", Dialog::JsConsole),
        ("js-prefs", Dialog::JsPrefs),
        ("doc-scripts", Dialog::DocScripts(Some(String::new()))),
        ("media-list", Dialog::MediaList),
        ("new-folder", Dialog::NewFolder),
        ("folder-filter", Dialog::FolderFilter(None)),
        ("combine", Dialog::Combine),
        ("picture-settings", Dialog::PictureSettings),
        ("split-pages", Dialog::SplitPages),
        ("redaction-look", Dialog::RedactionLook),
        ("redaction-code", Dialog::RedactionCode { set: true }),
        (
            "find-replace",
            Dialog::FindReplace {
                find: "quick brown".into(),
            },
        ),
        ("recovery", Dialog::Recovery),
        ("calc-order", Dialog::CalcOrder),
        ("note-icon", Dialog::NoteIcon(None)),
        ("spell-check", Dialog::SpellCheck),
        ("security-policies", Dialog::SecurityPolicies),
        ("customize-qat", Dialog::CustomizeQat),
        ("ocr", Dialog::Ocr { pictures_only: false }),
        ("ocr-pictures", Dialog::Ocr { pictures_only: true }),
        (
            "ocr-text",
            Dialog::OcrText {
                text: "The quick brown fox jumps over the lazy dog.\nIl gatto \u{e8} sul tavolo.".into(),
            },
        ),
        (
            "barcode",
            Dialog::Barcode {
                page: 0,
                rect: [100.0, 100.0, 300.0, 180.0],
            },
        ),
        (
            "barcodes",
            Dialog::Barcodes(vec![
                folitecta_core::FoundBarcode {
                    page: 0,
                    kind: "QR Code".into(),
                    text: "https://github.com/KvasFan/Folitecta/releases/tag/v0.0.10-with-a-long-address"
                        .into(),
                    rect: [100.0, 100.0, 200.0, 200.0],
                },
                folitecta_core::FoundBarcode {
                    page: 3,
                    kind: "EAN-13".into(),
                    text: "5901234123457".into(),
                    rect: [100.0, 300.0, 300.0, 360.0],
                },
            ]),
        ),
        (
            "replace-text",
            Dialog::ReplaceText {
                page: 0,
                start: 0,
                end: 7,
                note: false,
            },
        ),
    ];
    // The list of source files shows the files of the explorer step.
    let files = dir.join("explorer-files");
    crate::commands::sources::add_paths(
        st,
        ["report.pdf", "picture.png", "notes.txt"]
            .iter()
            .map(|n| files.join(n))
            .filter(|p| p.exists())
            .collect(),
    );
    // The editor shows the table being edited.
    *crate::commands::tables::TABLE_EDIT.write() = Some(crate::commands::tables::TableEdit {
        page: 0,
        index: 0,
        table: folitecta_core::tables::Table::new(4, 3),
        sel: (1, 1, 0, 1),
        fill: [0.9, 0.93, 1.0],
    });
    for lang in [crate::i18n::Lang::It, crate::i18n::Lang::En] {
        st.ui_mut().lang = lang;
        let code = lang.code();
        total += check(st, &dir, &format!("960x620-{code}-home"), log).await;
        for (name, d) in dialogs.iter() {
            st.set_dialog(d.clone());
            total += check(st, &dir, &format!("960x620-{code}-dialog-{name}"), log).await;
        }
        st.set_dialog(Dialog::None);
        total += mail_dialog(st, &dir, code, true, log).await;
    }
    *crate::commands::tables::TABLE_EDIT.write() = None;
    *crate::commands::sources::SOURCES.write() = Default::default();
    // Every theme, with a menu open: its text must stand out from its background.
    for theme in [Theme::Dark, Theme::Black, Theme::White] {
        st.ui_mut().theme = theme;
        st.set_open_menu(Some("edit".into()));
        total += check(st, &dir, &format!("960x620-{}", theme.attr()), log).await;
        st.set_open_menu(None);
        if theme == Theme::Dark {
            total += mail_dialog(st, &dir, theme.attr(), false, log).await;
        }
    }
    st.ui_mut().theme = Theme::Light;
    st.ui_mut().ribbon = false;
    total += check(st, &dir, "960x620-classic", log).await;
    // Every toolbar of the classic interface, then the touch interface.
    let all: Vec<String> = crate::ui::chrome::toolbars::TOOLBARS
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();
    crate::ui::chrome::toolbars::set_visible_toolbars(st, &all);
    total += check(st, &dir, "960x620-classic-all-toolbars", log).await;
    st.ui_mut().values.remove("toolbars");
    st.ui_mut().ribbon = true;
    st.set_value("ui.touch", "1");
    total += check(st, &dir, "960x620-touch", log).await;
    st.set_value("ui.touch", "");
    // A split window, side by side and one view above the other.
    crate::commands::dispatch(st, "window.split.vert");
    total += check(st, &dir, "960x620-split-side", log).await;
    crate::commands::dispatch(st, "window.split.horz");
    total += check(st, &dir, "960x620-split-stacked", log).await;
    crate::commands::dispatch(st, "window.removeSplit");
    // The Arrange tab of a selected comment.
    let comment = st
        .read_active(|t| {
            let annots = t.doc.annotations(t.page).ok()?;
            annots
                .iter()
                .find(|a| a.subtype != "Widget" && a.subtype != "Popup")
                .map(|a| (t.page, a.index))
        })
        .flatten();
    if let Some((p, i)) = comment {
        st.with_active(|t| t.set_chosen(p, &[i]));
        st.set_ribbon_tab("ctx.arrange".into());
        total += check(st, &dir, "960x620-arrange-tab", log).await;
        st.with_active(|t| t.set_chosen(p, &[]));
        st.set_ribbon_tab("home".into());
    }
    // The tools as icons alone (the names hidden), in both languages.
    st.set_option("view.ribbonLabels", false);
    for lang in [crate::i18n::Lang::It, crate::i18n::Lang::En] {
        st.ui_mut().lang = lang;
        total += check(st, &dir, &format!("960x620-{}-tool-icons", lang.code()), log).await;
    }
    st.set_option("view.ribbonLabels", true);
    // Horizontal scrolling with the pan and zoom window.
    st.set_option("view.layout.ribbon", true);
    st.set_option("view.panzoom", true);
    total += check(st, &dir, "960x620-horizontal-panzoom", log).await;
    st.set_option("view.layout.ribbon", false);
    st.set_option("view.panzoom", false);
    // Window resize back and forth must keep the layout consistent.
    for (w, h) in [(1500.0, 950.0), (1000.0, 700.0), (1366.0, 768.0)] {
        set_size(w, h);
        pause(900).await;
        total += check(st, &dir, &format!("resize-{}x{}", w as u32, h as u32), log).await;
    }
    st.set_option("view.ribbonLabels", false);
    total += check(st, &dir, "1366x768-tool-icons", log).await;
    st.set_option("view.ribbonLabels", true);
    total
}
