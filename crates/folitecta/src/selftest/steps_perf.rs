//! Performance probe: how long the interface takes to show sharp pages and
//! to follow the pointer, on two documents generated in memory — a typical
//! one (50 pages of text set in embedded fonts, two comments a page and a
//! figure every fifth page), a long one (1000 pages with two comments
//! each) and, for scrolling only, 40 pages of drawings that take long to
//! draw. Run it alone (`FOLITECTA_SELFTEST_ONLY=step:perf`, three runs of
//! every scenario, medians reported); a full run does every scenario once.
//!
//! What is measured, in the page with `performance.now()`:
//! - opening: from adding the document until the pages in the window show
//!   pictures sharp enough for the pixel ratio, with the thumbnails pane
//!   hidden and shown (and the first thumbnails);
//! - scrolling at fit width, smoothly (2000 px/s for 4 s) and in a fling
//!   (12000 px/s for 1.5 s), each frame moving by the time since the
//!   previous one: frame intervals (on time within 20 ms, over 20 and 50 ms,
//!   the longest stall), how long a page in the window was blank or not
//!   sharp, then the time until the pages are sharp;
//! - scrolling 120 px a frame for 2 s: frame intervals, the events sent
//!   synchronously to the program; a jump far down; wheel events;
//! - zooming: six Ctrl+wheel steps 30 ms apart, a dynamic zoom drag, a drag
//!   of the pane edge at fit-width zoom: time from the last input until the
//!   pages have their final size and sharp pictures;
//! - tools: text selection, hand and comment drags (time each move takes to
//!   dispatch, frame intervals), and after the comment is dropped the time
//!   until the page shows its new picture; moves over the ribbon and the
//!   thumbnails;
//! - in the program: renders of the components, pictures asked and drawn.
//!
//! The scenarios pass unless something does not happen within its time
//! limit; the numbers depend on the machine and go to `perf.json`.
//! Synthetic events do not scroll and do not reach the compositor, so real
//! wheel scrolling still needs a run by hand.

use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use super::*;
use crate::perf::Snapshot;
use crate::state::{Layout, Zoom};

/// Installed once in the page: counts and times the events sent
/// synchronously to the program and the batches of changes the page
/// applies, records frame intervals on demand, and tells when the pages in
/// the window show sharp pictures.
const PROBE_JS: &str = r#"
if (!window.__foliPerf) {
  const it = window.interpreter;
  const P = window.__foliPerf = { rec: false, last: 0, pending: [] };
  P.reset = () => { Object.assign(P, { xhr: 0, xhrMs: 0, xhrMax: 0, names: {}, edits: 0, editKB: 0, editWait: 0, editApply: 0, editApplyMax: 0, frames: [] }); };
  P.reset();
  if (it && typeof it.sendSerializedEvent === 'function') {
    const send = it.sendSerializedEvent;
    it.sendSerializedEvent = function (body) {
      const t = performance.now();
      try { return send.call(this, body); } finally {
        const d = performance.now() - t;
        P.xhr++; P.xhrMs += d; if (d > P.xhrMax) P.xhrMax = d;
        const n = body && body.name || '?'; P.names[n] = (P.names[n] || 0) + 1;
      }
    };
  }
  if (it && typeof it.enqueueBytes === 'function' && typeof it.flushQueuedBytes === 'function') {
    const enq = it.enqueueBytes;
    it.enqueueBytes = function (b) { P.pending.push(performance.now()); P.editKB += (b && b.byteLength || 0) / 1024; return enq.call(this, b); };
    const flush = it.flushQueuedBytes;
    it.flushQueuedBytes = function () {
      const t = performance.now();
      for (const a of P.pending) { P.editWait += t - a; P.edits++; }
      P.pending = [];
      try { return flush.call(this); } finally { const d = performance.now() - t; P.editApply += d; if (d > P.editApplyMax) P.editApplyMax = d; }
    };
  }
  const loop = t => { if (P.rec) { if (P.last) P.frames.push(t - P.last); P.last = t; } else P.last = 0; requestAnimationFrame(loop); };
  requestAnimationFrame(loop);
  const q = (v, p) => v.length ? v[Math.min(v.length - 1, Math.floor(p * (v.length - 1)))] : 0;
  P.stats = () => {
    const f = P.frames.slice().sort((a, b) => a - b);
    return { xhr: P.xhr, xhr_ms: P.xhrMs, xhr_max: P.xhrMax, edits: P.edits, edit_kb: P.editKB, edit_wait_ms: P.editWait, edit_apply_ms: P.editApply, edit_apply_max: P.editApplyMax,
             frames: f.length, frame_p50: q(f, 0.5), frame_p95: q(f, 0.95), frame_max: f.length ? f[f.length - 1] : 0, frames_over_50: f.filter(x => x > 50).length };
  };
  P.moves = v => { const s = v.slice().sort((a, b) => a - b); return { moves: s.length, move_p50: q(s, 0.5), move_p95: q(s, 0.95), move_max: s.length ? s[s.length - 1] : 0 }; };
  // The pages in the window: laid out at `zoom` (when given, for pages
  // `pagew` points wide) and showing pictures with enough pixels.
  // The pages of the canvas in the window (pages are found by a binary
  // search: they follow each other from top to bottom).
  P.visible = () => {
    const c = document.getElementById('canvas'); if (!c) return [];
    const v = c.getBoundingClientRect(); const pages = c.getElementsByClassName('page');
    let lo = 0, hi = pages.length;
    while (lo < hi) { const mid = (lo + hi) >> 1; if (pages[mid].getBoundingClientRect().bottom <= v.top) lo = mid + 1; else hi = mid; }
    const out = [];
    for (let i = lo; i < pages.length; i++) {
      const r = pages[i].getBoundingClientRect();
      if (r.top >= v.bottom) break;
      if (r.bottom > v.top && r.right > v.left && r.left < v.right) out.push(pages[i]);
    }
    return out;
  };
  // What a page shows: 0 nothing (blank), 1 a picture with too few pixels
  // (a preview, or the picture of another zoom), 2 a sharp picture.
  P.pageState = (pg, dpr) => {
    const img = pg.querySelector('.page-img');
    if (img && img.naturalWidth > 0) {
      const w = img.getBoundingClientRect().width;
      return img.complete && img.naturalWidth >= w * dpr * 0.9 ? 2 : 1;
    }
    const pv = pg.querySelector('.page-preview');
    if (pv && (pv.naturalWidth > 0 || pv.style.backgroundImage)) return 1;
    return 0;
  };
  // The pages in the window: laid out at `zoom` (when given, for pages
  // `pagew` points wide) and showing sharp pictures.
  P.sharpNow = (zoom, pagew) => {
    const dpr = window.devicePixelRatio || 1;
    const shown = P.visible();
    for (const pg of shown) {
      if (zoom > 0 && Math.abs(pg.getBoundingClientRect().width - pagew * zoom) > 1.5) return false;
      if (P.pageState(pg, dpr) < 2) return false;
    }
    return shown.length > 0;
  };
  // Scroll the canvas at `speed` pixels a second for `ms` (each frame moves
  // by the time since the previous one, so a late frame shows as a jump):
  // frame intervals, how long a page in the window was blank or not sharp.
  P.scrollRun = async (speed, ms) => {
    const c = document.getElementById('canvas'); const dpr = window.devicePixelRatio || 1;
    P.reset();
    let blank = 0, unsharp = 0, blankFrames = 0, n = 0;
    const frames = [];
    const top0 = c.scrollTop; let pos = top0;
    let last = await new Promise(r => requestAnimationFrame(r)); const t0 = last;
    for (;;) {
      const now = await new Promise(r => requestAnimationFrame(r));
      const dt = now - last; last = now;
      frames.push(dt); n++;
      let worst = 2;
      for (const pg of P.visible()) worst = Math.min(worst, P.pageState(pg, dpr));
      if (worst === 0) { blank += dt; blankFrames++; }
      if (worst < 2) unsharp += dt;
      if (now - t0 >= ms) break;
      pos += speed * dt / 1000;
      c.scrollTop = pos;
      if (c.scrollTop + c.clientHeight >= c.scrollHeight - 2) break;
    }
    const s = frames.slice().sort((a, b) => a - b);
    const q = p => s.length ? s[Math.min(s.length - 1, Math.floor(p * (s.length - 1)))] : 0;
    const took = last - t0;
    const st = P.stats();
    return { frames: n, frame_p50: q(0.5), frame_p95: q(0.95), longest_stall: s.length ? s[s.length - 1] : 0,
             frames_over_20: s.filter(x => x > 20).length, frames_over_50: s.filter(x => x > 50).length,
             on_time_pct: n ? 100 * s.filter(x => x <= 20).length / n : 0,
             blank_ms: blank, blank_frames: blankFrames, unsharp_ms: unsharp,
             px_per_s: took > 0 ? (c.scrollTop - top0) * 1000 / took : 0, ran_ms: took,
             xhr: st.xhr, xhr_ms: st.xhr_ms, edits: st.edits, edit_apply_ms: st.edit_apply_ms };
  };
  // Why the pages are not sharp (for the report).
  P.diag = () => {
    const c = document.getElementById('canvas'); if (!c) return 'no canvas';
    const v = c.getBoundingClientRect(); const dpr = window.devicePixelRatio || 1; const all = c.querySelectorAll('.page'); const f = all[0] ? all[0].getBoundingClientRect() : null; const out = ['dpr ' + dpr + ' view ' + Math.round(v.left) + ',' + Math.round(v.top) + '..' + Math.round(v.right) + ',' + Math.round(v.bottom) + ' pages ' + all.length + ' first ' + (f ? Math.round(f.left) + ',' + Math.round(f.top) + ' ' + Math.round(f.width) + 'x' + Math.round(f.height) : '-') + ' scroll ' + c.scrollTop + '/' + c.scrollHeight + ' html ' + c.innerHTML.slice(0, 300)];
    for (const pg of c.querySelectorAll('.page')) {
      const r = pg.getBoundingClientRect();
      if (r.bottom <= v.top || r.top >= v.bottom || r.right <= v.left || r.left >= v.right) continue;
      const img = pg.querySelector('.page-img');
      out.push(pg.id + ' ' + Math.round(r.width) + 'x' + Math.round(r.height) + (img ? ' img ' + img.complete + ' ' + img.naturalWidth + ' css ' + Math.round(img.getBoundingClientRect().width) + ' ' + img.getAttribute('src') : ' no img'));
      if (out.length > 4) break;
    }
    return out.join('; ');
  };
  P.waitFor = (t0, limit, ok) => new Promise(res => {
    const tick = () => { const now = performance.now(); if (ok()) res(now - t0); else if (now - t0 > limit) res(-1); else requestAnimationFrame(tick); };
    tick();
  });
  P.waitSharp = (t0, limit, zoom, pagew) => P.waitFor(t0, limit, () => P.sharpNow(zoom, pagew));
  P.frame = () => new Promise(r => requestAnimationFrame(() => r()));
  P.at = (el, x, y) => { const r = el.getBoundingClientRect(); return { clientX: r.left + x, clientY: r.top + y }; };
  P.mouse = (target, type, p, buttons, extra) => target.dispatchEvent(new MouseEvent(type, Object.assign({ bubbles: true, cancelable: true, button: 0, buttons }, p, extra || {})));
  // A drag: press at `from`, one move a frame along `path`, release; the
  // time each move takes to dispatch (the synchronous events included).
  P.drag = async (first, el, from, path) => {
    const down0 = performance.now();
    P.mouse(first, 'mousedown', from, 1);
    const down = performance.now() - down0;
    const times = [];
    P.reset(); P.rec = true;
    const t0 = performance.now();
    for (const p of path) {
      await P.frame();
      const t = performance.now();
      P.mouse(el, 'mousemove', p, 1);
      times.push(performance.now() - t);
    }
    await P.frame();
    P.mouse(el, 'mouseup', path[path.length - 1], 0);
    P.tUp = performance.now();
    P.rec = false;
    return Object.assign({ down_ms: down, total_ms: P.tUp - t0 }, P.moves(times), P.stats());
  };
}
return true;
"#;

/// A number of a script's answer.
fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

/// The medians of the numbers of several runs (other values from the first).
fn medians(runs: &[Map<String, Value>]) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(first) = runs.first() else {
        return out;
    };
    for (k, v) in first {
        if v.is_number() {
            let mut xs: Vec<f64> = runs
                .iter()
                .filter_map(|r| r.get(k).and_then(Value::as_f64))
                .collect();
            xs.sort_by(f64::total_cmp);
            let m = xs.get(xs.len() / 2).copied().unwrap_or(f64::NAN);
            out.insert(k.clone(), json!((m * 10.0).round() / 10.0));
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Numbers as `key value` pairs for the report.
fn brief(m: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| m.get(*k).map(|v| format!("{k} {v}")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The counters of the program and of the render threads of the active
/// document since `before`.
fn counted(st: AppState, before: (Snapshot, folitecta_core::RenderStats)) -> Map<String, Value> {
    let ui = Snapshot::now().since(before.0);
    let rs = st.read_active(|t| t.render.stats()).unwrap_or_default();
    let mut m = Map::new();
    m.insert("page_views".into(), json!(ui.page_views));
    m.insert("thumb_panes".into(), json!(ui.thumbnail_panes));
    m.insert("status_bars".into(), json!(ui.status_bars));
    m.insert("ribbons".into(), json!(ui.ribbons));
    m.insert("pictures_asked".into(), json!(ui.page_requests));
    m.insert("pictures_sent".into(), json!(ui.page_pictures));
    m.insert("answers_other".into(), json!(ui.page_other));
    m.insert("sent_mb".into(), json!(ui.page_bytes as f64 / 1048576.0));
    m.insert("rendered".into(), json!(rs.rendered - before.1.rendered));
    m.insert("reused".into(), json!(rs.reused - before.1.reused));
    m.insert("dropped".into(), json!(rs.dropped - before.1.dropped));
    m.insert(
        "render_ms".into(),
        json!((rs.render_us - before.1.render_us) as f64 / 1000.0),
    );
    m
}

fn mark(st: AppState) -> (Snapshot, folitecta_core::RenderStats) {
    (
        Snapshot::now(),
        st.read_active(|t| t.render.stats()).unwrap_or_default(),
    )
}

/// Run a script and return its answer as an object (empty on failure).
async fn obj(js: &str) -> Map<String, Value> {
    match eval(js).await {
        Ok(Value::Object(m)) => m,
        Ok(other) => {
            let mut m = Map::new();
            m.insert("answer".into(), other);
            m
        }
        Err(e) => {
            let mut m = Map::new();
            m.insert("error".into(), json!(e));
            m
        }
    }
}

/// Wait until the pages in the window are sharp (from now); milliseconds,
/// or -1 after `limit_ms`.
async fn wait_sharp(limit_ms: u32) -> f64 {
    let v = eval(&format!(
        "return await window.__foliPerf.waitSharp(performance.now(), {}, 0, 0);",
        limit_ms.min(25_000)
    ))
    .await;
    let ms = v.ok().and_then(|v| v.as_f64()).unwrap_or(-1.0);
    if ms < 0.0 {
        let why = eval("return window.__foliPerf.diag();").await.unwrap_or_default();
        progress::line(&format!("  the pages are not sharp: {why}"));
    }
    ms
}

/// The zoom of the active document once it stops changing.
async fn settled_zoom(st: AppState) -> f32 {
    let mut last = crate::commands::current_zoom(st);
    let mut same = 0;
    for _ in 0..100 {
        pause(40).await;
        let z = crate::commands::current_zoom(st);
        if (z - last).abs() < 1e-4 {
            same += 1;
            if same >= 2 {
                break;
            }
        } else {
            same = 0;
        }
        last = z;
    }
    last
}

/// Resident memory (MB) of this process and of the web processes it started.
fn memory_mb() -> (f64, f64) {
    fn status(pid: &str) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/status")).ok()
    }
    fn field(s: &str, key: &str) -> Option<String> {
        s.lines()
            .find(|l| l.starts_with(key))
            .map(|l| l[key.len()..].trim().to_string())
    }
    let rss = |s: &str| {
        field(s, "VmRSS:")
            .and_then(|v| v.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()))
            .unwrap_or(0.0)
            / 1024.0
    };
    let me = std::process::id().to_string();
    let own = status("self").map(|s| rss(&s)).unwrap_or(0.0);
    let mut web = 0.0;
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let pid = e.file_name().to_string_lossy().to_string();
            if !pid.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let Some(s) = status(&pid) else { continue };
            if !field(&s, "Name:").is_some_and(|n| n.starts_with("WebKit")) {
                continue;
            }
            // One of ours: this process is among its parents.
            let mut parent = field(&s, "PPid:");
            for _ in 0..6 {
                match parent {
                    Some(p) if p == me => {
                        web += rss(&s);
                        break;
                    }
                    Some(p) if p != "0" && p != "1" => {
                        parent = status(&p).and_then(|ps| field(&ps, "PPid:"));
                    }
                    _ => break,
                }
            }
        }
    }
    (own, web)
}

struct Probe<'a, 'b> {
    cx: &'a mut Ctx<'b>,
    reps: usize,
    results: Map<String, Value>,
}

impl Probe<'_, '_> {
    /// Record a scenario of document `doc`: the medians of its runs.
    fn record(&mut self, doc: &str, name: &str, runs: Vec<Map<String, Value>>, ok: bool, keys: &[&str]) {
        let m = medians(&runs);
        let detail = brief(&m, keys);
        self.cx.record(&format!("perf {doc}: {name}"), ok, detail);
        let entry = self
            .results
            .entry(doc.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(d) = entry {
            d.insert(name.to_string(), Value::Object(m));
        }
    }
}

/// Add a document as a new tab and make it the active one; returns its id.
fn add_tab(st: AppState, bytes: Vec<u8>) -> Option<u64> {
    let doc = folitecta_core::Document::from_bytes(bytes, None, None).ok()?;
    let id = st.alloc_id();
    st.docs_mut().push(DocTab::new(id, doc));
    let n = st.docs.read().len();
    st.set_active(n - 1);
    Some(id)
}

fn close_tab(st: AppState, id: u64, back_to: usize) {
    st.docs_mut().retain(|t| t.id != id);
    st.set_active(back_to);
}

/// Scroll the canvas so page `page` is at the top.
async fn scroll_to_page(page: usize) {
    let _ = eval(&format!(
        "const c = document.getElementById('canvas'), p = document.getElementById('pg-{page}'); if (c && p) c.scrollTop = p.offsetTop - 16; return true;"
    ))
    .await;
}

/// Show the document at zoom `zoom` with the page `page` at the top, sharp.
async fn reset_view_at(st: AppState, page: usize, zoom: f32) {
    st.with_active(|t| {
        if t.zoom != Zoom::Fixed(zoom) {
            t.zoom = Zoom::Fixed(zoom);
        }
    });
    pause(150).await;
    scroll_to_page(page).await;
    pause(150).await;
    let _ = wait_sharp(10_000).await;
}

/// Show the document at zoom 1 with the page `page` at the top, sharp.
async fn reset_view(st: AppState, page: usize) {
    reset_view_at(st, page, 1.0).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let alone = std::env::var("FOLITECTA_SELFTEST_ONLY").is_ok_and(|v| v == "step:perf");
    let reps = if alone { 3 } else { 1 };
    let back_to = st.active_index().unwrap_or(0);
    let pane_before = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    let installed = eval(PROBE_JS)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut probe = Probe {
        cx,
        reps,
        results: Map::new(),
    };
    // What the numbers were measured on.
    let env = obj(
        "const c = document.createElement('canvas'); const gl = c.getContext('webgl'); let gpu = ''; if (gl) { const d = gl.getExtension('WEBGL_debug_renderer_info'); gpu = d ? gl.getParameter(d.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER); } return { dpr: window.devicePixelRatio, w: window.innerWidth, h: window.innerHeight, webgl: gpu, agent: navigator.userAgent };",
    )
    .await;
    let mut machine = env.clone();
    machine.insert(
        "load_average".into(),
        json!(
            std::fs::read_to_string("/proc/loadavg")
                .unwrap_or_default()
                .trim()
        ),
    );
    for var in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if let Ok(v) = std::env::var(var) {
            machine.insert(var.into(), json!(v));
        }
    }
    machine.insert(
        "processors".into(),
        json!(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)),
    );
    probe
        .results
        .insert("machine".into(), Value::Object(machine.clone()));
    probe.cx.record(
        "perf: the probe is installed in the page",
        installed,
        format!("{machine:?}"),
    );

    // The drawings: 40 pages that take a long time to draw (2000 curves and
    // 800 transparent shapes each), scrolled only.
    for (doc, pages) in [("typical", 50usize), ("long", 1000usize), ("drawings", 40usize)] {
        let bytes = match doc {
            "typical" => folitecta_core::testdocs::report_doc(pages),
            "long" => folitecta_core::testdocs::long_doc(pages),
            _ => folitecta_core::testdocs::vector_doc(pages),
        };
        document_scenarios(&mut probe, doc, pages, bytes).await;
    }

    // Leave the window as it was.
    st.set_tool(Tool::Hand);
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = pane_before.0;
        ui.left_pane = pane_before.1;
    }
    st.set_active(back_to);
    st.with_active(|t| t.zoom = Zoom::Fixed(1.0));
    let results = Value::Object(std::mem::take(&mut probe.results));
    let _ = std::fs::write(
        probe.cx.dir.join("perf.json"),
        serde_json::to_string_pretty(&results).unwrap_or_default(),
    );
    pause(500).await;
}

/// Whether the scenarios of `group` run: every group, or those named in
/// `FOLITECTA_PERF_GROUPS` (comma separated: open, scroll, zoom, tools).
fn wanted(group: &str) -> bool {
    std::env::var("FOLITECTA_PERF_GROUPS")
        .map(|v| v.split(',').any(|g| g.trim() == group))
        .unwrap_or(true)
}

async fn document_scenarios(p: &mut Probe<'_, '_>, doc: &str, pages: usize, bytes: Vec<u8>) {
    let st = p.cx.st;
    let reps = p.reps;
    let scroll_only = doc == "drawings";
    let back_to = st.active_index().unwrap_or(0);
    // The pages in the middle.
    let mid = pages / 2;

    if wanted("open") {
        // 1. Opening, with the thumbnails pane hidden and shown.
        for thumbs in [false, true] {
            let mut runs = Vec::new();
            let mut ok = true;
            for _ in 0..reps {
                {
                    let mut ui = st.ui_mut();
                    ui.show_left_pane = thumbs;
                    ui.left_pane = "thumbnails".into();
                }
                pause(300).await;
                let before = Snapshot::now();
                let started = Instant::now();
                let Some(id) = add_tab(st, bytes.clone()) else {
                    ok = false;
                    break;
                };
                let added = started.elapsed().as_secs_f64() * 1000.0;
                let sharp = wait_sharp(25_000).await;
                let total = started.elapsed().as_secs_f64() * 1000.0;
                let mut m = Map::new();
                m.insert("add_tab_ms".into(), json!(added));
                m.insert(
                    "open_to_sharp_ms".into(),
                    json!(if sharp < 0.0 { -1.0 } else { total }),
                );
                if thumbs {
                    // The thumbnails the pane shows.
                    let first = obj(
                        "const P = window.__foliPerf; const t0 = performance.now(); const ms = await P.waitFor(t0, 25000, () => { const pane = document.querySelector('.pane-panel'); if (!pane) return false; const pr = pane.getBoundingClientRect(); const imgs = [...document.querySelectorAll('.thumbs .thumb img')].filter(i => { const r = i.getBoundingClientRect(); return r.bottom > pr.top && r.top < pr.bottom; }); return imgs.length > 0 && imgs.every(i => i.complete && i.naturalWidth > 0); }); return { thumbs_after_ms: ms };",
                    )
                    .await;
                    let after = num(&Value::Object(first), "thumbs_after_ms");
                    m.insert(
                        "thumbs_shown_ms".into(),
                        json!(if after < 0.0 {
                            -1.0
                        } else {
                            started.elapsed().as_secs_f64() * 1000.0
                        }),
                    );
                }
                let ui = Snapshot::now().since(before);
                m.insert("page_views".into(), json!(ui.page_views));
                m.insert("pictures_asked".into(), json!(ui.page_requests));
                ok &= sharp >= 0.0;
                runs.push(m);
                close_tab(st, id, back_to);
                pause(400).await;
            }
            let name = if thumbs {
                "open, thumbnails shown"
            } else {
                "open, thumbnails hidden"
            };
            p.record(
                doc,
                name,
                runs,
                ok,
                &[
                    "open_to_sharp_ms",
                    "add_tab_ms",
                    "thumbs_shown_ms",
                    "page_views",
                    "pictures_asked",
                ],
            );
        }
    }
    // The document stays open for the other scenarios, thumbnails hidden.
    st.ui_mut().show_left_pane = false;
    let Some(id) = add_tab(st, bytes.clone()) else {
        p.cx.record(&format!("perf {doc}: the document opens"), false, String::new());
        return;
    };
    st.set_tool(Tool::Hand);
    pause(500).await;
    reset_view(st, 0).await;
    let pagew = st
        .read_active(|t| t.page_sizes.first().map(|s| s.width))
        .flatten()
        .unwrap_or(595.0);

    if wanted("scroll") {
        // 2a. Smooth scrolling and flings at fit width (what a page shows when a
        // document opens): frame intervals, blank and blurred pages.
        for (name, speed, ms) in [
            ("smooth scroll 2000 px/s for 4 s", 2000, 4000),
            ("fling 12000 px/s for 1.5 s", 12000, 1500),
        ] {
            let mut runs = Vec::new();
            let mut ok = true;
            for r in 0..reps {
                st.with_active(|t| t.zoom = Zoom::FitWidth);
                pause(200).await;
                scroll_to_page(if pages > 100 { 100 + 150 * r } else { 2 * r }).await;
                let _ = wait_sharp(15_000).await;
                pause(300).await;
                let page_before = st.current_page();
                let before = mark(st);
                let mut m = obj(&format!(
                    "const P = window.__foliPerf; const r = await P.scrollRun({speed}, {ms}); r.sharp_after_ms = await P.waitSharp(performance.now(), 20000, 0, 0); return r;"
                ))
                .await;
                pause(300).await;
                m.extend(counted(st, before));
                m.insert(
                    "pages_passed".into(),
                    json!(st.current_page() as i64 - page_before as i64),
                );
                ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
                runs.push(m);
            }
            p.record(
                doc,
                name,
                runs,
                ok,
                &[
                    "on_time_pct",
                    "frames_over_20",
                    "frames_over_50",
                    "longest_stall",
                    "frame_p95",
                    "px_per_s",
                    "blank_ms",
                    "unsharp_ms",
                    "sharp_after_ms",
                    "xhr",
                    "page_views",
                    "pages_passed",
                    "pictures_asked",
                    "rendered",
                ],
            );
        }

        // 2. Scrolling 120 px a frame for 2 s, then until the pages are sharp.
        let mut runs = Vec::new();
        let mut ok = true;
        for r in 0..reps {
            reset_view(st, if pages > 100 { r * 100 } else { 0 }).await;
            let page_before = st.current_page();
            let before = mark(st);
            let mut m = obj(
                "const P = window.__foliPerf, c = document.getElementById('canvas'); P.reset(); P.rec = true; const t0 = performance.now(); let n = 0; await new Promise(res => { const step = () => { c.scrollTop += 120; n++; if (performance.now() - t0 < 2000) requestAnimationFrame(step); else res(); }; requestAnimationFrame(step); }); P.rec = false; const s = P.stats(); const sharp = await P.waitSharp(performance.now(), 20000, 0, 0); return Object.assign(s, { steps: n, sharp_after_ms: sharp });",
            )
            .await;
            pause(300).await;
            m.extend(counted(st, before));
            m.insert(
                "pages_passed".into(),
                json!(st.current_page() as i64 - page_before as i64),
            );
            ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
            runs.push(m);
        }
        p.record(
            doc,
            "scroll 120 px a frame for 2 s",
            runs,
            ok,
            &[
                "sharp_after_ms",
                "frame_p50",
                "frame_p95",
                "frame_max",
                "frames_over_50",
                "xhr",
                "xhr_ms",
                "edits",
                "edit_apply_ms",
                "page_views",
                "pages_passed",
                "rendered",
            ],
        );

        // 3. A jump far down (a scroll bar drag or End), then until sharp.
        let mut runs = Vec::new();
        let mut ok = true;
        for r in 0..reps {
            reset_view(st, 0).await;
            let target = (mid + r * 7).min(pages - 1);
            let before = mark(st);
            let mut m = obj(&format!(
                "const P = window.__foliPerf, c = document.getElementById('canvas'), pg = document.getElementById('pg-{target}'); P.reset(); const t0 = performance.now(); c.scrollTop = pg.offsetTop - 16; const sharp = await P.waitSharp(t0, 20000, 0, 0); return Object.assign(P.stats(), {{ sharp_after_ms: sharp }});"
            ))
            .await;
            // When the current page follows.
            let started = Instant::now();
            while st.current_page() != target && started.elapsed() < Duration::from_secs(5) {
                pause(20).await;
            }
            m.insert(
                "page_followed_ms".into(),
                json!(if st.current_page() == target {
                    started.elapsed().as_secs_f64() * 1000.0
                } else {
                    -1.0
                }),
            );
            m.extend(counted(st, before));
            ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
            runs.push(m);
        }
        p.record(
            doc,
            "jump far down",
            runs,
            ok,
            &[
                "sharp_after_ms",
                "page_followed_ms",
                "xhr",
                "page_views",
                "rendered",
                "pictures_asked",
            ],
        );

        // 4. Wheel events (they do not scroll: the cost of their listeners).
        let mut runs = Vec::new();
        for _ in 0..reps {
            reset_view(st, mid).await;
            let before = mark(st);
            let mut m = obj(
                "const P = window.__foliPerf, c = document.getElementById('canvas'); const r = c.getBoundingClientRect(); const el = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) || c; P.reset(); const times = []; for (let i = 0; i < 60; i++) { await new Promise(res => setTimeout(res, 16)); const t = performance.now(); el.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: 40, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 })); times.push(performance.now() - t); } return Object.assign(P.stats(), P.moves(times));",
            )
            .await;
            m.extend(counted(st, before));
            runs.push(m);
        }
        p.record(
            doc,
            "60 wheel events",
            runs,
            true,
            &["move_p50", "move_p95", "move_max", "xhr", "xhr_ms", "page_views"],
        );
    }
    if wanted("zoom") && !scroll_only {
        // 5. Zoom: six Ctrl+wheel steps 30 ms apart.
        let mut runs = Vec::new();
        let mut ok = true;
        for r in 0..reps {
            // A different start for every run: no picture of an earlier run is reused.
            reset_view_at(st, mid + r, 1.0 + 0.07 * r as f32).await;
            let before = mark(st);
            let mut m = obj(
                "const P = window.__foliPerf, c = document.getElementById('canvas'); const r = c.getBoundingClientRect(); const el = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 3) || c; P.reset(); const times = []; for (let i = 0; i < 6; i++) { if (i) await new Promise(res => setTimeout(res, 30)); const t = performance.now(); el.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, deltaY: -100, deltaMode: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height / 3 })); times.push(performance.now() - t); } P.tLast = performance.now(); return Object.assign(P.moves(times), { xhr: P.xhr });",
            )
            .await;
            let zoom = settled_zoom(st).await;
            let sharp = obj(&format!(
                "const P = window.__foliPerf; return {{ sharp_after_ms: await P.waitSharp(P.tLast, 20000, {zoom}, {pagew}) }};"
            ))
            .await;
            m.extend(sharp);
            pause(200).await;
            m.extend(counted(st, before));
            m.insert("zoom".into(), json!(zoom));
            ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
            runs.push(m);
        }
        p.record(
            doc,
            "zoom: six Ctrl+wheel steps",
            runs,
            ok,
            &[
                "sharp_after_ms",
                "zoom",
                "move_p50",
                "move_max",
                "page_views",
                "pictures_asked",
                "rendered",
                "dropped",
                "render_ms",
            ],
        );

        // 6. Dynamic zoom: a drag of 40 moves upwards.
        let mut runs = Vec::new();
        let mut ok = true;
        for r in 0..reps {
            reset_view_at(st, mid + r, 1.0 + 0.07 * r as f32).await;
            st.set_tool(Tool::ZoomDyn);
            pause(300).await;
            let before = mark(st);
            let cur = st.current_page();
            let mut m = obj(&format!(
                "const P = window.__foliPerf, el = document.querySelector('#pg-{cur} .page-inner'); if (!el) return {{ error: 'no page' }}; const path = []; for (let i = 1; i <= 40; i++) path.push(P.at(el, 200, 300 - i * 2)); return await P.drag(el, el, P.at(el, 200, 300), path);"
            ))
            .await;
            let zoom = settled_zoom(st).await;
            let sharp = obj(&format!(
                "const P = window.__foliPerf; return {{ sharp_after_ms: await P.waitSharp(P.tUp, 20000, {zoom}, {pagew}) }};"
            ))
            .await;
            m.extend(sharp);
            pause(200).await;
            m.extend(counted(st, before));
            m.insert("zoom".into(), json!(zoom));
            ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
            runs.push(m);
            st.set_tool(Tool::Hand);
        }
        p.record(
            doc,
            "dynamic zoom drag",
            runs,
            ok,
            &[
                "sharp_after_ms",
                "zoom",
                "move_p50",
                "move_p95",
                "frame_p95",
                "page_views",
                "pictures_asked",
                "rendered",
                "dropped",
            ],
        );

        // 7. A drag of the left pane's edge at fit-width zoom: 40 frames of 3 px.
        let mut runs = Vec::new();
        let mut ok = true;
        for r in 0..reps {
            st.ui_mut().show_left_pane = true;
            st.ui_mut().left_pane = "bookmarks".into();
            st.with_active(|t| t.zoom = Zoom::FitWidth);
            pause(400).await;
            scroll_to_page(mid).await;
            let _ = wait_sharp(15_000).await;
            let before = mark(st);
            // A different path for every run: no picture of an earlier run is reused.
            let mut m = obj(&format!(
                "const P = window.__foliPerf, main = document.querySelector('.main'); if (!main) return {{ error: 'no main' }}; const w0 = parseFloat(getComputedStyle(main).getPropertyValue('--pane-w')) || 240; P.reset(); P.rec = true; for (let i = 1; i <= 40; i++) {{ await P.frame(); main.style.setProperty('--pane-w', (w0 + {off} + 3 * i) + 'px'); }} P.tLast = performance.now(); P.rec = false; P.paneW0 = w0; return P.stats();",
                off = 2 * r
            ))
            .await;
            let zoom = settled_zoom(st).await;
            let sharp = obj(&format!(
                "const P = window.__foliPerf; return {{ sharp_after_ms: await P.waitSharp(P.tLast, 20000, {zoom}, {pagew}) }};"
            ))
            .await;
            m.extend(sharp);
            pause(200).await;
            m.extend(counted(st, before));
            ok &= num(&Value::Object(m.clone()), "sharp_after_ms") >= 0.0;
            runs.push(m);
            let _ = eval("const P = window.__foliPerf, main = document.querySelector('.main'); if (main && P.paneW0) main.style.setProperty('--pane-w', P.paneW0 + 'px'); return true;").await;
            st.ui_mut().show_left_pane = false;
            pause(300).await;
        }
        p.record(
            doc,
            "pane edge drag at fit width",
            runs,
            ok,
            &[
                "sharp_after_ms",
                "frame_p95",
                "frame_max",
                "xhr",
                "page_views",
                "pictures_asked",
                "rendered",
                "dropped",
            ],
        );
    }
    if wanted("tools") && !scroll_only {
        // 8. Text selection: a drag over the lines of a page.
        let mut runs = Vec::new();
        let mut ok = true;
        let (x0, y0, lh) = if pages == 50 {
            (52.0f32, 75.0f32, 12.5f32)
        } else {
            (62.0, 71.0, 13.0)
        };
        for _ in 0..reps {
            reset_view(st, mid).await;
            st.set_tool(Tool::Select);
            pause(300).await;
            let before = mark(st);
            let cur = st.current_page();
            let mut m = obj(&format!(
                "const P = window.__foliPerf, el = document.querySelector('#pg-{cur} .page-inner'); if (!el) return {{ error: 'no page' }}; const path = []; for (let i = 1; i <= 60; i++) path.push(P.at(el, {x0} + (i % 20) * 22, {y0} + Math.floor(i / 20) * {lh})); const r = await P.drag(el, el, P.at(el, {x0}, {y0}), path); r.shown_after_ms = await P.waitFor(P.tUp, 10000, () => document.querySelectorAll('#pg-{cur} .sel').length > 0); return r;"
            ))
            .await;
            pause(200).await;
            m.extend(counted(st, before));
            let selected = st
                .read_active(|t| t.text_sel.map(|s| s.range().1 - s.range().0))
                .flatten()
                .unwrap_or(0);
            m.insert("chars".into(), json!(selected));
            ok &= selected > 0;
            runs.push(m);
            st.with_active(|t| t.text_sel = None);
            st.set_tool(Tool::Hand);
        }
        p.record(
            doc,
            "text selection drag",
            runs,
            ok,
            &[
                "down_ms",
                "move_p50",
                "move_p95",
                "move_max",
                "frame_p95",
                "shown_after_ms",
                "xhr",
                "page_views",
                "edits",
                "edit_apply_ms",
                "chars",
            ],
        );

        // 9. Hand tool: a drag that scrolls the pages.
        let mut runs = Vec::new();
        let mut ok = true;
        for _ in 0..reps {
            reset_view(st, mid).await;
            st.set_tool(Tool::Hand);
            pause(200).await;
            let before = mark(st);
            let cur = st.current_page();
            let mut m = obj(&format!(
                "const P = window.__foliPerf, c = document.getElementById('canvas'), el = document.querySelector('#pg-{cur} .page-inner'); if (!el) return {{ error: 'no page' }}; const top0 = c.scrollTop; const path = []; for (let i = 1; i <= 40; i++) path.push(P.at(el, 330, 600 - i * 5)); const r = await P.drag(el, el, P.at(el, 330, 600), path); r.scrolled = c.scrollTop - top0; return r;"
            ))
            .await;
            pause(200).await;
            m.extend(counted(st, before));
            ok &= num(&Value::Object(m.clone()), "scrolled") > 50.0;
            runs.push(m);
        }
        p.record(
            doc,
            "hand tool drag",
            runs,
            ok,
            &[
                "down_ms",
                "move_p50",
                "move_p95",
                "frame_p95",
                "frame_max",
                "xhr",
                "page_views",
                "scrolled",
            ],
        );

        // 10. Moving a comment (the rectangle of the page), then until the page
        // shows its new picture.
        let mut runs = Vec::new();
        let mut ok = true;
        for _ in 0..reps {
            reset_view(st, mid).await;
            st.set_tool(Tool::EditAnnots);
            pause(400).await;
            let cur = st.current_page();
            let before = mark(st);
            let rev_before = st.read_active(|t| t.doc.revision()).unwrap_or(0);
            let mut m = obj(&format!(
                "const P = window.__foliPerf, el = document.querySelector('#pg-{cur} .page-inner'); if (!el) return {{ error: 'no page' }}; const img = el.querySelector('.page-img'); const src0 = img ? img.src : ''; const from = P.at(el, 200, 392); const first = document.elementFromPoint(from.clientX, from.clientY) || el; const path = []; for (let i = 1; i <= 30; i++) path.push(P.at(el, 200 + i, 392 + i)); const r = await P.drag(first, el, from, path); r.target = String(first.className); r.new_picture_ms = await P.waitFor(P.tUp, 15000, () => {{ const i = document.querySelector('#pg-{cur} .page-img'); return i && i.src !== src0 && i.complete && i.naturalWidth > 0; }}); return r;"
            ))
            .await;
            pause(300).await;
            m.extend(counted(st, before));
            let moved = st.read_active(|t| t.doc.revision()).unwrap_or(0) != rev_before;
            m.insert("moved".into(), json!(moved));
            ok &= moved && num(&Value::Object(m.clone()), "new_picture_ms") >= 0.0;
            runs.push(m);
            crate::commands::dispatch(st, "undo");
            st.set_tool(Tool::Hand);
            pause(400).await;
        }
        p.record(
            doc,
            "comment drag",
            runs,
            ok,
            &[
                "down_ms",
                "move_p50",
                "move_p95",
                "move_max",
                "new_picture_ms",
                "xhr",
                "page_views",
                "thumb_panes",
                "pictures_asked",
                "rendered",
                "moved",
                "target",
            ],
        );

        // 11. The pointer over the ribbon and the thumbnails (nothing to do).
        let mut runs = Vec::new();
        for _ in 0..reps {
            st.ui_mut().show_left_pane = true;
            st.ui_mut().left_pane = "thumbnails".into();
            pause(500).await;
            let before = mark(st);
            let mut m = obj(
                "const P = window.__foliPerf; const els = [...document.querySelectorAll('.ribbon-body .rbtn, .thumbs .thumb')].slice(0, 40); P.reset(); const times = []; for (let i = 0; i < 100; i++) { const el = els[i % els.length]; if (!el) break; const r = el.getBoundingClientRect(); await P.frame(); const t = performance.now(); el.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: r.left + 3, clientY: r.top + 3 })); times.push(performance.now() - t); } return Object.assign(P.stats(), P.moves(times));",
            )
            .await;
            m.extend(counted(st, before));
            runs.push(m);
            st.ui_mut().show_left_pane = false;
            pause(200).await;
        }
        p.record(
            doc,
            "pointer over the ribbon and thumbnails",
            runs,
            true,
            &["xhr", "xhr_ms", "move_p50", "move_max", "page_views"],
        );
    }
    let (own, web) = memory_mb();
    let mut m = Map::new();
    m.insert("ui_process_mb".into(), json!(own));
    m.insert("web_processes_mb".into(), json!(web));
    p.record(
        doc,
        "memory",
        vec![m],
        true,
        &["ui_process_mb", "web_processes_mb"],
    );

    st.set_tool(Tool::Hand);
    close_tab(st, id, back_to);
    st.with_active(|t| t.layout = Layout::Continuous);
    pause(600).await;
}
