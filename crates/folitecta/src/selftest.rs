//! Interface self-test (`--ui-selftest <dir>`): resizes the window through a set
//! of sizes and UI states, audits the layout for overlapping or clipped
//! elements and stores a report plus a screenshot of every state.

use std::path::PathBuf;
use std::time::Duration;

use dioxus::desktop::LogicalSize;
use dioxus::prelude::*;

use crate::state::{AppState, Dialog, DocTab, TextSel, Theme, Tool};

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
const rows = ['.menubar', '.ribbon-tabs', '.ribbon-right', '.ribbon-body', '.ribbon-group-items', '.statusbar', '.statusbar .seg', '.pane-toolbar', '.search-nav', '.search-opts', '.dialog-foot', '.field-grid', '.radio-row', '.label-row', '.doc-tabs', '.pane-header', '.welcome .actions', '.check-grid', '.stamp-grid', '.row', '.sel-toolbar', '.list-item .title', '.list-item .meta'];
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
return JSON.stringify({ vw, vh, problems });
"#;

async fn pause(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn set_size(w: f64, h: f64) {
    let window = dioxus::desktop::window();
    window.set_maximized(false);
    window.set_inner_size(LogicalSize::new(w, h));
}

/// A sample document with text, marks, comments and bookmarks.
fn sample_document() -> folitecta_core::Result<folitecta_core::Document> {
    let para = "Folitecta renders, annotates and organizes PDF documents. The quick brown fox jumps over the lazy dog. \
Pack my box with five dozen liquor jugs. Visit https://github.com/KvasExpert/Folitecta for the source code. ";
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
async fn check(st: AppState, dir: &std::path::Path, name: &str, log: &mut Vec<serde_json::Value>) -> usize {
    pause(700).await;
    let audit = match document::eval(AUDIT_JS).await {
        Ok(v) => v
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or(v),
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    };
    let count = audit
        .get("problems")
        .and_then(|p| p.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let shot = dir.join(format!("{name}.png"));
    let capture = capture_window(&shot);
    log.push(serde_json::json!({ "state": name, "audit": audit, "screenshot": shot.display().to_string(), "capture": capture.err() }));
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
    match document::eval(&js).await {
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

/// Exercise the canvas tools with simulated input; returns the number of failures.
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
    let mut failures = 0;
    let mut record = |name: &str, pass: bool, detail: String| {
        if !pass {
            failures += 1;
        }
        log.push(serde_json::json!({ "interaction": name, "pass": pass, "detail": detail }));
    };

    // 1. Text selection by dragging with the text tool.
    st.set_tool(Tool::Select);
    pause(300).await;
    let r = pointer_path(0, z, &line((58.0, 62.0), (300.0, 64.0), 8), false).await;
    pause(500).await;
    let sel = st
        .read_active(|t| {
            t.text_sel.map(|s| {
                (
                    s.page,
                    s.range(),
                    t.doc.text_range(s.page, s.range().0, s.range().1),
                )
            })
        })
        .flatten();
    record(
        "select text by dragging",
        sel.as_ref()
            .map(|(p, (a, b), _)| *p == 0 && b > a)
            .unwrap_or(false),
        format!("{r}; {sel:?}"),
    );

    // 2. Highlight the selection from the floating toolbar command.
    let before = annots_on(st, 0).len();
    crate::commands::dispatch(st, "textsel.addHighlight");
    pause(500).await;
    let after = annots_on(st, 0);
    record(
        "highlight selected text",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Highlight").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 3. Draw a rectangle.
    st.set_tool(Tool::Rect);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r = pointer_path(0, z, &line((100.0, 300.0), (260.0, 380.0), 6), false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    let rect_ok = after.len() == before + 1 && after.last().map(|a| a.subtype == "Square").unwrap_or(false);
    record(
        "draw a rectangle",
        rect_ok,
        format!("{r}; {before} -> {}; tool now {:?}", after.len(), (st.tool)()),
    );

    // 4. Move it with the comment editing tool.
    st.set_tool(Tool::EditAnnots);
    pause(500).await;
    let size = st.read_active(|t| t.page_sizes[0]).unwrap();
    if let Some(a) = annots_on(st, 0).last().cloned() {
        let d = folitecta_core::marks::pdf_rect_to_display(&size, a.rect);
        let c = ((d[0] + d[2]) / 2.0, (d[1] + d[3]) / 2.0);
        let r = pointer_path(0, z, &line(c, (c.0 + 40.0, c.1 + 30.0), 6), true).await;
        pause(600).await;
        let moved = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| folitecta_core::marks::pdf_rect_to_display(&size, x.rect));
        let ok = moved
            .map(|m| ((m[0] - d[0]) - 40.0).abs() < 1.5 && ((m[1] - d[1]) - 30.0).abs() < 1.5)
            .unwrap_or(false);
        record(
            "move a comment by dragging",
            ok,
            format!("{r}; before {d:?} after {moved:?}"),
        );

        // 5. Resize it from the bottom-right handle.
        let selected = st.read_active(|t| t.selected_annot).flatten();
        if let Some(m) = moved {
            let corner = (m[2], m[3]);
            let r = pointer_path(0, z, &line(corner, (corner.0 + 30.0, corner.1 + 20.0), 5), true).await;
            pause(600).await;
            let resized = annots_on(st, 0)
                .into_iter()
                .find(|x| x.index == a.index)
                .map(|x| folitecta_core::marks::pdf_rect_to_display(&size, x.rect));
            let ok = resized
                .map(|n| {
                    ((n[2] - n[0]) - (m[2] - m[0]) - 30.0).abs() < 1.5
                        && ((n[3] - n[1]) - (m[3] - m[1]) - 20.0).abs() < 1.5
                })
                .unwrap_or(false);
            record(
                "resize a comment with a handle",
                ok,
                format!("{r}; selected {selected:?}; before {m:?} after {resized:?}"),
            );
        }

        // 6. Nudge with the keyboard, then delete with the Delete key.
        st.with_active(|t| t.selected_annot = Some((0, a.index)));
        let before_rect = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| x.rect);
        let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true, bubbles: true }));").await;
        pause(500).await;
        let after_rect = annots_on(st, 0)
            .into_iter()
            .find(|x| x.index == a.index)
            .map(|x| x.rect);
        let ok = match (before_rect, after_rect) {
            (Some(b), Some(n)) => ((n[0] - b[0]) - 10.0).abs() < 0.6,
            _ => false,
        };
        record(
            "nudge the selected comment with Shift+Right",
            ok,
            format!("{before_rect:?} -> {after_rect:?}"),
        );
        let before = annots_on(st, 0).len();
        st.with_active(|t| t.selected_annot = Some((0, a.index)));
        let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }));").await;
        pause(500).await;
        record(
            "delete the selected comment with the Delete key",
            annots_on(st, 0).len() + 1 == before,
            format!("{before} -> {}", annots_on(st, 0).len()),
        );
    } else {
        record(
            "move a comment by dragging",
            false,
            "no annotation to move".into(),
        );
    }

    // 7. Stamp with a single click.
    st.set_value("stamp.kind", "text");
    st.set_value("stamp.text", "REVIEWED");
    st.set_tool(Tool::Stamp);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let r = pointer_path(0, z, &[(300.0, 500.0), (300.0, 500.0)], false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    record(
        "place a stamp with a click",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Stamp").unwrap_or(false),
        format!("{r}; {before} -> {}", after.len()),
    );

    // 8. Freehand ink.
    st.set_tool(Tool::Ink);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let wave: Vec<(f32, f32)> = (0..20)
        .map(|i| (120.0 + i as f32 * 8.0, 600.0 + ((i as f32) * 0.8).sin() * 20.0))
        .collect();
    let r = pointer_path(0, z, &wave, false).await;
    pause(600).await;
    let after = annots_on(st, 0);
    record(
        "draw with the pencil",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Ink").unwrap_or(false),
        format!("{r}; {before} -> {}", after.len()),
    );

    // 9. Sticky note opens its dialog.
    st.set_tool(Tool::Note);
    pause(300).await;
    let _ = pointer_path(0, z, &[(400.0, 200.0), (400.0, 200.0)], false).await;
    pause(500).await;
    let dialog = (st.dialog)();
    record(
        "sticky note tool opens the note dialog",
        matches!(dialog, Dialog::Note { page: 0, .. }),
        format!("{dialog:?}"),
    );
    st.set_dialog(Dialog::None);

    // 10. Text highlight tool marks text directly.
    st.set_tool(Tool::TextHighlight);
    pause(300).await;
    let before = annots_on(st, 0).len();
    let _ = pointer_path(0, z, &line((58.0, 120.0), (250.0, 122.0), 6), false).await;
    pause(700).await;
    let after = annots_on(st, 0);
    record(
        "highlight text tool",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Highlight").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 11. Snapshot copies an area to the clipboard.
    st.set_tool(Tool::Snapshot);
    pause(300).await;
    let _ = pointer_path(0, z, &line((50.0, 50.0), (250.0, 150.0), 4), false).await;
    pause(900).await;
    let png = crate::clipboard::get_png();
    record(
        "snapshot tool copies an image",
        png.as_ref().map(|p| p.len() > 100).unwrap_or(false),
        format!("{} bytes", png.map(|p| p.len()).unwrap_or(0)),
    );

    // 12. Polygon with three clicks and a double click.
    crate::commands::dispatch(st, "tool.annot.polygon");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(100.0, 650.0), (200.0, 640.0), (160.0, 720.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(150).await;
    }
    let _ = document::eval("const el = document.querySelector('#pg-0 .page-inner'); const r = el.getBoundingClientRect(); el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: r.left + 160, clientY: r.top + 720 }));").await;
    pause(600).await;
    let after = annots_on(st, 0);
    record(
        "polygon with clicks and a double click",
        after.len() == before + 1 && after.last().map(|a| a.subtype == "Polygon").unwrap_or(false),
        format!("{before} -> {}", after.len()),
    );

    // 13. Distance measurement by dragging (72 pt = 25.4 mm).
    crate::commands::dispatch(st, "tool.annot.distance");
    pause(300).await;
    let _ = pointer_path(0, z, &line((300.0, 650.0), (372.0, 650.0), 5), false).await;
    pause(600).await;
    let last = annots_on(st, 0).last().cloned();
    record(
        "distance measurement",
        last.as_ref()
            .map(|a| a.subtype == "Line" && a.contents.ends_with("mm"))
            .unwrap_or(false),
        format!("{:?}", last.map(|a| a.contents)),
    );

    // 14. Area with Enter to finish, cloud with Escape to cancel.
    crate::commands::dispatch(st, "tool.annot.area");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(420.0, 640.0), (500.0, 640.0), (500.0, 700.0), (420.0, 700.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(120).await;
    }
    let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));").await;
    pause(600).await;
    let after = annots_on(st, 0);
    record(
        "area measurement finished with Enter",
        after.len() == before + 1
            && after
                .last()
                .map(|a| a.contents.contains('\u{b2}'))
                .unwrap_or(false),
        format!(
            "{before} -> {}; {:?}",
            after.len(),
            after.last().map(|a| a.contents.clone())
        ),
    );
    crate::commands::dispatch(st, "tool.annot.cloud");
    pause(300).await;
    let before = annots_on(st, 0).len();
    for p in [(100.0, 750.0), (200.0, 750.0)] {
        let _ = pointer_path(0, z, &[p, p], false).await;
        pause(120).await;
    }
    let _ = document::eval("document.querySelector('.app').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));").await;
    pause(400).await;
    record(
        "Escape cancels a shape being drawn",
        annots_on(st, 0).len() == before && st.drawing.peek().is_none(),
        String::new(),
    );

    // 15. Callout: drag from the pointed spot to where the box goes, then type.
    crate::commands::dispatch(st, "tool.annot.callout");
    pause(300).await;
    let _ = pointer_path(0, z, &line((80.0, 400.0), (250.0, 330.0), 5), false).await;
    pause(500).await;
    let opened = matches!((st.dialog)(), Dialog::Callout { page: 0, .. });
    st.set_dialog(Dialog::None);
    record("callout tool opens its text dialog", opened, String::new());

    // 16. Eraser cuts a pencil stroke.
    crate::commands::dispatch(st, "tool.annot.pencil");
    pause(300).await;
    let stroke: Vec<(f32, f32)> = (0..30).map(|i| (100.0 + i as f32 * 8.0, 560.0)).collect();
    let _ = pointer_path(0, z, &stroke, false).await;
    pause(500).await;
    let inks_before = annots_on(st, 0).iter().filter(|a| a.subtype == "Ink").count();
    crate::commands::dispatch(st, "tool.annot.eraser");
    pause(300).await;
    let _ = pointer_path(0, z, &line((220.0, 540.0), (220.0, 580.0), 4), false).await;
    pause(600).await;
    let ink_paths = st
        .read_active(|t| {
            let a = t
                .doc
                .annotations(0)
                .ok()?
                .into_iter()
                .rfind(|a| a.subtype == "Ink")?;
            let d = t.doc.inner().get_dictionary(a.id?).ok()?;
            Some(d.get(b"InkList").ok()?.as_array().ok()?.len())
        })
        .flatten();
    record(
        "eraser splits a pencil stroke",
        inks_before >= 1 && ink_paths == Some(2),
        format!("ink comments {inks_before}, strokes after erasing {ink_paths:?}"),
    );

    // 17. Insert text and replace text open their dialogs.
    crate::commands::dispatch(st, "tool.annot.insertText");
    pause(300).await;
    let _ = pointer_path(0, z, &[(70.0, 92.0), (70.0, 92.0)], false).await;
    pause(500).await;
    let insert = matches!((st.dialog)(), Dialog::InsertText { page: 0, .. });
    st.set_dialog(Dialog::None);
    record(
        "insert text tool opens its dialog",
        insert,
        format!("{:?}", (st.dialog)()),
    );
    crate::commands::dispatch(st, "tool.annot.replaceText");
    pause(300).await;
    let _ = pointer_path(0, z, &line((58.0, 92.0), (200.0, 92.0), 6), false).await;
    pause(500).await;
    let replace = matches!((st.dialog)(), Dialog::ReplaceText { page: 0, .. });
    st.set_dialog(Dialog::None);
    record("replace text tool opens its dialog", replace, String::new());

    // 18. Style editor: select the polygon and change its border colour.
    st.set_tool(Tool::EditAnnots);
    let poly = annots_on(st, 0)
        .into_iter()
        .find(|a| a.subtype == "Polygon")
        .map(|a| a.index);
    if let Some(i) = poly {
        st.with_active(|t| t.selected_annot = Some((0, i)));
        st.ui_mut().show_right_pane = true;
        pause(700).await;
        let _ = document::eval("const c = document.querySelector('.style-editor input[type=color]'); if (c) { c.value = '#00aa00'; c.dispatchEvent(new Event('change', { bubbles: true })); }").await;
        pause(700).await;
        let color = st
            .read_active(|t| t.doc.annotation_style(0, i).ok().and_then(|s| s.color))
            .flatten();
        record(
            "style editor changes the border colour",
            color
                .map(|c| c[0] < 0.05 && (c[1] - 0.667).abs() < 0.02)
                .unwrap_or(false),
            format!("{color:?}"),
        );
        let _ = capture_window(&dir.join("interaction-style-editor.png"));
        st.ui_mut().show_right_pane = false;
    } else {
        record(
            "style editor changes the border colour",
            false,
            "no polygon".into(),
        );
    }

    // 19. Form fields: create a text field and a check box, fill them on the page.
    crate::commands::dispatch(st, "tool.editFields.mode.text");
    pause(300).await;
    let _ = pointer_path(0, z, &line((300.0, 150.0), (500.0, 172.0), 4), false).await;
    pause(600).await;
    let fields = st.read_active(|t| t.doc.form_fields()).unwrap_or_default();
    record(
        "text field created by dragging",
        fields.len() == 1 && (st.tool)() == Tool::EditFields,
        format!("{} field(s), tool {:?}", fields.len(), (st.tool)()),
    );
    crate::commands::dispatch(st, "tool.editFields.mode.check");
    pause(300).await;
    let _ = pointer_path(0, z, &[(300.0, 190.0), (300.0, 190.0)], false).await;
    pause(600).await;
    let fields = st.read_active(|t| t.doc.form_fields()).unwrap_or_default();
    record(
        "check box created with a click",
        fields.len() == 2,
        format!("{:?}", fields.iter().map(|f| f.kind.label()).collect::<Vec<_>>()),
    );
    // Properties pane: rename the selected check box.
    st.ui_mut().show_right_pane = true;
    pause(700).await;
    let _ = document::eval("const n = document.querySelector('.field-editor input.wide'); if (n) { n.value = 'Accept'; n.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(600).await;
    let renamed = st
        .read_active(|t| t.doc.form_fields().iter().any(|f| f.name == "Accept"))
        .unwrap_or(false);
    record(
        "field renamed in the properties pane",
        renamed,
        format!(
            "{:?}",
            st.read_active(|t| t
                .doc
                .form_fields()
                .into_iter()
                .map(|f| f.name)
                .collect::<Vec<_>>())
        ),
    );
    let _ = capture_window(&dir.join("interaction-field-editor.png"));
    st.ui_mut().show_right_pane = false;
    // Fill the text field on the page and tick the box.
    crate::commands::dispatch(st, "tool.hand");
    pause(500).await;
    let _ = document::eval(
        "const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[0]) h[0].click();",
    )
    .await;
    pause(500).await;
    let editing = st.editing_field.peek().is_some();
    let _ = document::eval("const i = document.querySelector('#pg-0 .field-input'); if (i) { i.value = 'Ada Lovelace'; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); }").await;
    pause(600).await;
    let _ = document::eval(
        "const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[1]) h[1].click();",
    )
    .await;
    pause(600).await;
    let values: Vec<String> = st
        .read_active(|t| t.doc.form_fields().into_iter().map(|f| f.value).collect())
        .unwrap_or_default();
    record(
        "fill a text field and tick a check box on the page",
        editing && values == ["Ada Lovelace", "Yes"],
        format!("editor opened {editing}; values {values:?}"),
    );
    let _ = capture_window(&dir.join("interaction-filled-form.png"));

    // 20. Undo everything added above.
    st.set_tool(Tool::Hand);
    for _ in 0..30 {
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
    let mut log: Vec<serde_json::Value> = Vec::new();
    let mut total = 0;
    // FOLITECTA_SELFTEST_ONLY=print runs only the print step (for debugging).
    let only = std::env::var("FOLITECTA_SELFTEST_ONLY").ok();
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
    }
    // Printed layout of the first pages (emulated on screen; the system dialog is modal).
    set_size(1280.0, 800.0);
    pause(800).await;
    let pages: Vec<crate::state::PrintPage> = st
        .read_active(|t| {
            (0..3.min(t.doc.page_count()))
                .map(|p| (t.id, p, t.revision))
                .collect()
        })
        .unwrap_or_default();
    let ready = crate::files::prepare_print(st, pages, 100.0, true, true).await;
    let _ = document::eval("document.documentElement.classList.add('emulate-print'); window.scrollTo(0, 0);")
        .await;
    pause(800).await;
    let shot = dir.join("print-layout.png");
    let capture = capture_window(&shot);
    let _ = document::eval("document.documentElement.classList.remove('emulate-print');").await;
    let pass = ready == 3 && capture.is_ok();
    if !pass {
        total += 1;
    }
    log.push(serde_json::json!({ "interaction": "print layout with 3 rendered pages", "pass": pass, "detail": format!("{ready} pages ready"), "screenshot": shot.display().to_string(), "capture": capture.err() }));
    let report = serde_json::json!({ "total_problems": total, "states": log });
    let _ = std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap_or_default(),
    );
    println!(
        "ui self-test finished: {total} problem(s); report in {}",
        dir.display()
    );
    std::process::exit(if total == 0 { 0 } else { 3 });
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
        st.ui_mut().left_pane = "thumbnails".into();
        // A menu with a submenu opened on its first entry that has children,
        // and a ribbon overflow dropdown near the right edge.
        st.set_open_menu(Some("help".into()));
        pause(400).await;
        let _ = document::eval(
            "const it = [...document.querySelectorAll('.menu-dropdown .menu-item')].find(e => e.querySelector('.chev')); if (it) { const r = it.getBoundingClientRect(); it.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, clientX: r.left + 20, clientY: r.top + 8 })); it.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false, clientX: r.left + 20, clientY: r.top + 8 })); it.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left + 20, clientY: r.top + 8 })); }",
        )
        .await;
        total += check(st, &dir, &format!("{tag}-menu-help-submenu"), log).await;
        st.close_menus();
        let _ = document::eval(
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
    ];
    for lang in [crate::i18n::Lang::It, crate::i18n::Lang::En] {
        st.ui_mut().lang = lang;
        let code = lang.code();
        total += check(st, &dir, &format!("960x620-{code}-home"), log).await;
        for (name, d) in dialogs.iter() {
            st.set_dialog(d.clone());
            total += check(st, &dir, &format!("960x620-{code}-dialog-{name}"), log).await;
        }
        st.set_dialog(Dialog::None);
    }
    st.ui_mut().theme = Theme::Dark;
    total += check(st, &dir, "960x620-dark", log).await;
    st.ui_mut().theme = Theme::Light;
    st.ui_mut().ribbon = false;
    total += check(st, &dir, "960x620-classic", log).await;
    st.ui_mut().ribbon = true;
    // Window resize back and forth must keep the layout consistent.
    for (w, h) in [(1500.0, 950.0), (1000.0, 700.0), (1366.0, 768.0)] {
        set_size(w, h);
        pause(900).await;
        total += check(st, &dir, &format!("resize-{}x{}", w as u32, h as u32), log).await;
    }
    total
}
