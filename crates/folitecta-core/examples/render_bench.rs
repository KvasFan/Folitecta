//! Measures how long the pictures of the pages take, through the code path of
//! the interface: the documents are generated here (text, vector drawings,
//! photos, scanned pages, a long document with comments) plus
//! `docs/sample.pdf`, so no file of the user is ever read.
//!
//! ```text
//! cargo build --release -p folitecta-core --example render_bench
//! target/release/examples/render_bench [--quick] [--reps N] [--out DIR] [--only NAME]
//! ```
//!
//! Run it on a quiet machine, or pinned to a few cores (`taskset -c 12-15`
//! on Linux) when other work runs; note the pinning next to the numbers.
//!
//! Sections: for each document (`--only text|vector|photo|scan|sample`)
//! opening (lopdf model and renderer parse), the time of a page split into
//! rasterizing and PNG encoding for every zoom of the interface (100, 200 and
//! 400 % at a pixel ratio of 1 and 2, and the thumbnails), tiles and low
//! resolution pictures, other ways of sending the pixels to the window
//! (encoding and decoding time, bytes) and the cost of an edit; then the
//! render service (`--only service`: 1 to 4 threads, a burst of zoom steps
//! without cancelling stale requests, thumbnails asked before the pages) and
//! the document calls every page of the view makes each time the interface
//! draws it again (`--only model`). `--out DIR` writes the documents and the
//! encoded pictures, for other renderers and decoders.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Instant;

use folitecta_core::{Document, RenderService, Renderer};
use hayro::hayro_interpret::{InterpreterSettings, TransformExt};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::vello_cpu::{Pixels, Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render, render_into};
use image::ExtendedColorType;
use lopdf::{Dictionary, Object, Stream, dictionary};

/// The largest edge of a page picture (as in `render.rs`).
const MAX_EDGE: f32 = 12_000.0;
/// The largest scale the canvas asks for (`ui/canvas/mod.rs`: 6000 milli).
const UI_MAX_SCALE: f32 = 6.0;

struct Opts {
    quick: bool,
    reps: usize,
    out: Option<PathBuf>,
    only: Option<String>,
}

fn main() {
    let mut opts = Opts {
        quick: false,
        reps: 3,
        out: None,
        only: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--quick" => {
                opts.quick = true;
                opts.reps = 1;
            }
            "--reps" => opts.reps = args.next().and_then(|v| v.parse().ok()).unwrap_or(3).max(1),
            "--out" => opts.out = args.next().map(PathBuf::from),
            "--only" => opts.only = args.next(),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    if let Some(dir) = &opts.out {
        std::fs::create_dir_all(dir).expect("output folder");
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("# Folitecta render benchmark");
    println!(
        "processors available to this process: {cores} (the interface starts {} render threads per document here)",
        cores.div_ceil(2).clamp(1, 3)
    );
    println!("times are medians of {} runs, in milliseconds\n", opts.reps);

    let started = Instant::now();
    let mut docs: Vec<(&str, Vec<u8>)> = vec![
        ("text", text_doc(if opts.quick { 4 } else { 12 })),
        ("vector", vector_doc(if opts.quick { 2 } else { 4 })),
        ("photo", photo_doc(2)),
        ("scan", scan_doc(if opts.quick { 2 } else { 4 })),
    ];
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample.pdf");
    if let Ok(bytes) = std::fs::read(&sample) {
        docs.push(("sample", bytes));
    }
    println!(
        "documents generated in {:.0} ms\n",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let wanted = |name: &str| opts.only.as_deref().is_none_or(|o| o == name);
    if let Some(dir) = &opts.out {
        // The documents themselves, for other renderers and decoders.
        for (name, bytes) in &docs {
            let _ = std::fs::write(dir.join(format!("{name}.pdf")), bytes);
        }
    }

    for (name, bytes) in &docs {
        if !wanted(name) {
            continue;
        }
        reset_peak_rss();
        let bytes = Arc::new(bytes.clone());
        println!("## {name}");
        let pdf = open_section(bytes.clone(), &opts);
        scale_section(&pdf, bytes.clone(), &opts);
        tile_section(&pdf, &opts);
        encoder_section(name, &pdf, &opts);
        edit_section(bytes.clone(), &opts);
        if let Some(mb) = peak_rss_mb() {
            println!("peak resident memory during this document: {mb:.0} MB\n");
        }
    }
    if wanted("service") {
        reset_peak_rss();
        let text = Arc::new(text_doc(12));
        let vector = Arc::new(vector_doc(4));
        println!("## render service");
        service_section("text", text.clone(), &opts);
        service_section("vector", vector.clone(), &opts);
        zoom_burst_section("text", text.clone(), &opts);
        zoom_burst_section("vector", vector.clone(), &opts);
        open_burst_section("text", text, &opts);
        open_burst_section("vector", vector, &opts);
        if let Some(mb) = peak_rss_mb() {
            println!("peak resident memory during this section: {mb:.0} MB\n");
        }
    }
    if wanted("model") {
        println!("## document calls made by every page of the view");
        let sizes: &[usize] = if opts.quick { &[50, 300] } else { &[50, 300, 1000] };
        for &pages in sizes {
            model_section(pages, &opts);
        }
    }
}

// ---- measuring ---------------------------------------------------------------------

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Median time of `reps` runs of `f` (milliseconds) and the last result.
fn timed<T>(reps: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = Vec::with_capacity(reps);
    let mut last = None;
    for _ in 0..reps.max(1) {
        let t = Instant::now();
        let value = f();
        times.push(ms_since(t));
        // The previous result is freed after the measure.
        last = Some(value);
    }
    (median(&mut times), last.expect("at least one run"))
}

/// Median of the times `f` measures itself.
fn median_of(reps: usize, mut f: impl FnMut() -> f64) -> f64 {
    let mut v: Vec<f64> = (0..reps.max(1)).map(|_| f()).collect();
    median(&mut v)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Peak resident memory of this process (Linux), in megabytes.
fn peak_rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

/// Start counting the peak resident memory again (Linux).
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

fn kib(n: usize) -> String {
    if n >= 10 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1048576.0)
    } else {
        format!("{:.0} KB", n as f64 / 1024.0)
    }
}

// ---- the steps of a render, as `render.rs` does them --------------------------------

/// The scale `render.rs` uses for a requested one (the largest edge is limited).
fn effective_scale(pdf: &Pdf, page: usize, scale: f32) -> f32 {
    let pages = pdf.pages();
    let (w, h) = pages[page].render_dimensions();
    scale.max(0.05).min(MAX_EDGE / w.max(h).max(1.0))
}

fn rasterize<'a>(pdf: &'a Pdf, cache: &RenderCache<'a>, page: usize, scale: f32) -> Pixmap {
    let scale = effective_scale(pdf, page, scale);
    let pages = pdf.pages();
    let settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };
    render(
        &pages[page],
        cache,
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &settings,
    )
}

/// A part of the page (`x`, `y`, `w`, `h` in pixels of the page at `scale`),
/// drawn on its own: what a tile or the loupe would need.
fn rasterize_tile<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    rect: (u16, u16, u16, u16),
) -> Pixmap {
    let pages = pdf.pages();
    let p = &pages[page];
    let (x, y, w, h) = rect;
    let mut ctx = RenderContext::new(w, h);
    let transform = Affine::translate((-(x as f64), -(y as f64)))
        * Affine::scale(scale as f64)
        * p.initial_transform(true).to_kurbo();
    render_into(
        p,
        cache,
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &mut ctx,
        transform,
    );
    ctx.flush();
    let mut pixmap = Pixmap::new(w, h);
    ctx.render_with(
        &mut pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::Clear(WHITE),
            ..Default::default()
        },
    );
    pixmap
}

/// Opaque pixels as RGB (what `encode_png` sends), else RGBA.
fn pixels_of(pixmap: Pixmap) -> (u32, u32, png::ColorType, Vec<u8>) {
    let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    match pixmap.try_take_rgb8(ImageAlphaType::Alpha) {
        Pixels::Rgb8(p) => (w, h, png::ColorType::Rgb, p),
        Pixels::Rgba8(p) => (w, h, png::ColorType::Rgba, p),
    }
}

fn png_bytes(
    pixels: &[u8],
    w: u32,
    h: u32,
    color: png::ColorType,
    deflate: png::DeflateCompression,
    filter: png::Filter,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(pixels.len() / 8);
    let mut encoder = png::Encoder::new(&mut data, w, h);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_deflate_compression(deflate);
    encoder.set_filter(filter);
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(pixels).expect("PNG data");
    writer.finish().expect("PNG end");
    data
}

/// The PNG of `render.rs` (`Compression::Fast`: fdeflate, adaptive filter).
fn png_now(pixels: &[u8], w: u32, h: u32, color: png::ColorType) -> Vec<u8> {
    png_bytes(
        pixels,
        w,
        h,
        color,
        png::DeflateCompression::FdeflateUltraFast,
        png::Filter::Adaptive,
    )
}

fn png_decode(data: &[u8]) -> usize {
    let mut reader = png::Decoder::new(Cursor::new(data))
        .read_info()
        .expect("PNG info");
    let mut buf = vec![0; reader.output_buffer_size().expect("PNG size")];
    reader.next_frame(&mut buf).expect("PNG frame");
    buf.len()
}

// ---- sections --------------------------------------------------------------------

fn open_section(bytes: Arc<Vec<u8>>, opts: &Opts) -> Pdf {
    let (load, doc) = timed(opts.reps, || {
        Document::from_bytes(bytes.to_vec(), None, None).expect("lopdf load")
    });
    let (parse, pdf) = timed(opts.reps, || Pdf::new(bytes.clone()).expect("hayro parse"));
    let pages = pdf.pages().len();
    let (sizes, _) = timed(opts.reps, || doc.page_sizes());
    let (renderer, _) = timed(opts.reps, || Renderer::new(bytes.clone()).expect("renderer"));
    println!(
        "{} pages, {}; open: editor model (lopdf, on the interface thread today) {load:.1} ms, \
         renderer parse (hayro) {parse:.2} ms, Renderer::new {renderer:.2} ms, page sizes {sizes:.2} ms",
        pages,
        kib(bytes.len())
    );
    pdf
}

/// Every zoom of the interface: rasterizing (a new render cache, as
/// `Renderer`, and the warm cache of a render thread), encoding, the public
/// `Renderer::render_page` for comparison, and the bytes.
fn scale_section(pdf: &Pdf, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let renderer = Renderer::new(bytes).expect("renderer");
    let pages = pdf.pages().len().min(if opts.quick { 1 } else { 3 });
    // zoom × pixel ratio of the canvas, as asked (the canvas limits the scale to 6).
    let combos: [(f32, f32); 7] = [
        (0.285, 1.0),
        (1.0, 1.0),
        (2.0, 1.0),
        (4.0, 1.0),
        (1.0, 2.0),
        (2.0, 2.0),
        (4.0, 2.0),
    ];
    println!(
        "| view | scale | pixels | raster cold | raster warm | PNG encode | total (Renderer) | PNG bytes | raw RGBA | MP/s |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (zoom, dpr) in combos {
        let scale = (zoom * dpr).min(UI_MAX_SCALE);
        let view = if zoom < 0.5 {
            "thumbnail".to_string()
        } else {
            format!("{:.0} % × {dpr:.0}", zoom * 100.0)
        };
        let (mut cold, mut warm, mut enc, mut total) = (vec![], vec![], vec![], vec![]);
        let (mut png_len, mut px) = (0usize, (0u32, 0u32));
        for page in 0..pages {
            let cache = RenderCache::new();
            for _ in 0..opts.reps {
                let t = Instant::now();
                let fresh = RenderCache::new();
                let pm = rasterize(pdf, &fresh, page, scale);
                cold.push(ms_since(t));
                drop(pm);
            }
            // The cache of a render thread after a first render of the page.
            drop(rasterize(pdf, &cache, page, scale));
            for _ in 0..opts.reps {
                let t = Instant::now();
                let pm = rasterize(pdf, &cache, page, scale);
                warm.push(ms_since(t));
                let (w, h, color, pixels) = pixels_of(pm);
                px = (w, h);
                let t = Instant::now();
                let data = png_now(&pixels, w, h, color);
                enc.push(ms_since(t));
                png_len = png_len.max(data.len());
            }
            for _ in 0..opts.reps {
                let t = Instant::now();
                let r = renderer.render_page(page, scale).expect("render_page");
                total.push(ms_since(t));
                assert_eq!((r.width, r.height), px);
            }
        }
        let raster = median(&mut warm);
        let mp = (px.0 as f64 * px.1 as f64) / 1e6;
        println!(
            "| {view} | {scale:.2} | {}×{} | {:.1} | {raster:.1} | {:.1} | {:.1} | {} | {} | {:.0} |",
            px.0,
            px.1,
            median(&mut cold),
            median(&mut enc),
            median(&mut total),
            kib(png_len),
            kib(px.0 as usize * px.1 as usize * 4),
            mp / (raster.max(0.001) / 1000.0)
        );
    }
    println!();
}

/// What drawing only what is seen would cost at a high zoom: a whole page at
/// 400 % on a pixel ratio of 2 against the part a 1600×1000 window shows,
/// the loupe (240×170 CSS pixels at 8×) and a picture at half the scale
/// shown first while the sharp one is made.
fn tile_section(pdf: &Pdf, opts: &Opts) {
    let page = 0;
    let scale = UI_MAX_SCALE;
    let s = effective_scale(pdf, page, scale);
    let pages = pdf.pages();
    let (pw, ph) = pages[page].render_dimensions();
    let (fw, fh) = ((pw * s) as u16, (ph * s) as u16);
    let cache = RenderCache::new();
    drop(rasterize(pdf, &cache, page, scale));
    let (full, _) = timed(opts.reps, || rasterize(pdf, &cache, page, scale));
    let view = (1600u16 * 2).min(fw);
    let view_h = (1000u16 * 2).min(fh);
    let rect = ((fw - view) / 2, (fh - view_h) / 2, view, view_h);
    let (tile, _) = timed(opts.reps, || rasterize_tile(pdf, &cache, page, s, rect));
    let ls = effective_scale(pdf, page, 8.0);
    let lrect = (
        ((pw * ls) as u16).saturating_sub(480) / 2,
        ((ph * ls) as u16).saturating_sub(340) / 2,
        480,
        340,
    );
    let (loupe_full, _) = timed(opts.reps, || rasterize(pdf, &cache, page, 8.0));
    let (loupe_tile, _) = timed(opts.reps, || rasterize_tile(pdf, &cache, page, ls, lrect));
    let (low, _) = timed(opts.reps, || rasterize(pdf, &cache, page, 0.5));
    let (thumb, _) = timed(opts.reps, || rasterize(pdf, &cache, page, 0.285));
    println!(
        "tiles (warm cache, page 1): whole page at scale {s:.2} ({fw}×{fh}) {full:.1} ms; the {}×{} px a 1600×1000 window shows {tile:.1} ms; \
         loupe: whole page at {ls:.2} {loupe_full:.1} ms, its 480×340 px {loupe_tile:.1} ms; low resolution first: scale 0.5 {low:.1} ms, thumbnail 0.285 {thumb:.1} ms\n",
        rect.2, rect.3
    );
}

/// Ways of sending the pixels of page 1 at scale 2 (100 % on a pixel ratio
/// of 2) to the window: encoding and decoding time (Rust decoders; the
/// window decodes with its own libraries) and bytes.
fn encoder_section(name: &str, pdf: &Pdf, opts: &Opts) {
    let cache = RenderCache::new();
    let (w, h, color, pixels) = pixels_of(rasterize(pdf, &cache, 0, 2.0));
    let channels = if color == png::ColorType::Rgb { 3 } else { 4 };
    let rgb: Vec<u8> = if channels == 3 {
        pixels.clone()
    } else {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect()
    };
    println!(
        "pixels sent for page 1 at scale 2 ({w}×{h}, {}):",
        if channels == 3 { "opaque, RGB" } else { "RGBA" }
    );
    println!("| format | encode | bytes | decode (Rust) |");
    println!("|---|---|---|---|");
    type Enc<'a> = Box<dyn Fn() -> Vec<u8> + 'a>;
    type Dec = fn(&[u8]) -> usize;
    let image_decode: Dec = |d| image::load_from_memory(d).expect("decode").as_bytes().len();
    let variants: Vec<(&str, &str, Enc, Dec)> = vec![
        (
            "PNG fdeflate, adaptive filter (today)",
            "fast.png",
            Box::new(|| png_now(&pixels, w, h, color)),
            png_decode,
        ),
        (
            "PNG fdeflate, Up filter",
            "up.png",
            Box::new(|| {
                png_bytes(
                    &pixels,
                    w,
                    h,
                    color,
                    png::DeflateCompression::FdeflateUltraFast,
                    png::Filter::Up,
                )
            }),
            png_decode,
        ),
        (
            "PNG stored (no compression)",
            "stored.png",
            Box::new(|| {
                png_bytes(
                    &pixels,
                    w,
                    h,
                    color,
                    png::DeflateCompression::NoCompression,
                    png::Filter::NoFilter,
                )
            }),
            png_decode,
        ),
        (
            "PNG zlib level 1, adaptive",
            "z1.png",
            Box::new(|| {
                png_bytes(
                    &pixels,
                    w,
                    h,
                    color,
                    png::DeflateCompression::Level(1),
                    png::Filter::Adaptive,
                )
            }),
            png_decode,
        ),
        (
            "BMP 24-bit",
            "bmp",
            Box::new(|| {
                let mut out = Vec::new();
                image::codecs::bmp::BmpEncoder::new(&mut out)
                    .encode(&rgb, w, h, ExtendedColorType::Rgb8)
                    .expect("BMP");
                out
            }),
            image_decode,
        ),
        (
            "WebP lossless",
            "webp",
            Box::new(|| {
                let mut out = Vec::new();
                image::codecs::webp::WebPEncoder::new_lossless(&mut out)
                    .encode(&rgb, w, h, ExtendedColorType::Rgb8)
                    .expect("WebP");
                out
            }),
            image_decode,
        ),
        (
            "raw RGBA (a canvas fed with putImageData)",
            "rgba",
            Box::new(|| {
                if channels == 4 {
                    pixels.clone()
                } else {
                    pixels
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .flat_map(|p| [p[0], p[1], p[2], 255])
                        .collect()
                }
            }),
            |d| d.to_vec().len(),
        ),
    ];
    for (label, ext, enc, dec) in variants {
        let (et, data) = timed(opts.reps, &enc);
        let (dt, _) = timed(opts.reps, || dec(&data));
        println!("| {label} | {et:.1} | {} | {dt:.1} |", kib(data.len()));
        if let Some(dir) = &opts.out {
            let _ = std::fs::write(dir.join(format!("{name}_s2.{ext}")), &data);
        }
    }
    if let Some(dir) = &opts.out {
        let _ = std::fs::write(dir.join(format!("{name}_s2.size")), format!("{w} {h}\n"));
    }
    println!();
}

/// An edit: the document is serialized again (`Document::commit`), every
/// render thread parses the new bytes and the pages are drawn again with an
/// empty cache.
fn edit_section(bytes: Arc<Vec<u8>>, opts: &Opts) {
    let mut doc = Document::from_bytes(bytes.to_vec(), None, None).expect("load");
    let (commit, _) = timed(opts.reps, || doc.commit().expect("commit"));
    let svc = RenderService::with_workers(bytes.clone(), 3);
    svc.render_blocking(0, 1.0).expect("render");
    let (warm, _) = timed(opts.reps, || {
        // A scale not in the memo of the thread.
        svc.render_blocking(0, 1.0 + rand_f32() * 0.5).expect("render")
    });
    let (after, _) = timed(opts.reps, || {
        svc.replace(doc.bytes());
        svc.render_blocking(0, 1.25).expect("render")
    });
    println!(
        "edit: Document::commit (serialize, on the interface thread) {commit:.1} ms; page 1 at 125 %: \
         {warm:.1} ms with a warm render thread, {after:.1} ms right after RenderService::replace\n"
    );
}

/// Every page of a document through the render service, as the canvas asks
/// for them, with 1 to 4 threads.
fn service_section(name: &str, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let pages = Renderer::new(bytes.clone()).expect("renderer").page_count();
    let scale = 2.5; // 125 % on a pixel ratio of 2
    println!("render service, {name} document, {pages} pages at scale {scale} (125 % × 2), cold threads:");
    println!("| threads | all pages | pages/s | first page |");
    println!("|---|---|---|---|");
    for n in [1usize, 2, 3, 4] {
        let mut all = vec![];
        let mut first = vec![];
        for _ in 0..opts.reps {
            let svc = RenderService::with_workers(bytes.clone(), n);
            let (tx, rx) = channel();
            let t = Instant::now();
            for p in 0..pages {
                let tx = tx.clone();
                svc.render(p, scale, move |r| {
                    let _ = tx.send((p, r.is_ok(), Instant::now()));
                });
            }
            let mut done: Vec<(usize, Instant)> = Vec::new();
            for _ in 0..pages {
                let (p, ok, at) = rx.recv().expect("reply");
                assert!(ok, "page {p}");
                done.push((p, at));
            }
            all.push(ms_since(t));
            first.push(
                done.iter()
                    .find(|(p, _)| *p == 0)
                    .map(|(_, at)| at.duration_since(t).as_secs_f64() * 1000.0)
                    .unwrap_or(0.0),
            );
        }
        let total = median(&mut all);
        println!(
            "| {n} | {total:.0} ms | {:.1} | {:.0} ms |",
            pages as f64 / (total / 1000.0),
            median(&mut first)
        );
    }
    println!();
}

/// Six steps of Ctrl+wheel zoom (×1.1 each) over three visible pages, faster
/// than the pages are drawn: the queue of each thread draws every stale
/// scale before the last one. Compared with drawing only the last step.
fn zoom_burst_section(name: &str, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let visible = [0usize, 1, 2];
    let steps: Vec<f32> = (0..6).map(|k| 2.0 * 1.1f32.powi(k)).collect();
    let last = *steps.last().expect("steps");
    let run = |scales: &[f32]| -> f64 {
        let svc = RenderService::with_workers(bytes.clone(), 3);
        // The threads have parsed the document and drawn the pages once.
        for p in visible {
            svc.render_blocking(p, 1.0).expect("render");
        }
        let (tx, rx) = channel();
        let t = Instant::now();
        let mut wanted = 0;
        for s in scales {
            for p in visible {
                let tx = tx.clone();
                let s = *s;
                svc.render(p, s, move |_| {
                    let _ = tx.send(s);
                });
                if s == last {
                    wanted += 1;
                }
            }
        }
        let mut got = 0;
        while got < wanted {
            if rx.recv().expect("reply") == last {
                got += 1;
            }
        }
        ms_since(t)
    };
    let fifo = median_of(opts.reps, || run(&steps));
    let only_last = median_of(opts.reps, || run(&[last]));
    println!(
        "zoom burst (3 threads, 3 visible {name} pages, 6 wheel steps from scale 2.0 to {last:.2}): \
         sharp pages after {fifo:.0} ms when every step is drawn (today), {only_last:.0} ms when only the last one is\n"
    );
}

/// A document opens with the thumbnails pane shown: the pane asks for its
/// thumbnails (scale 0.285) in the same queues as the canvas, before the two
/// pages of the canvas (125 % on a pixel ratio of 2). Time until those two
/// are drawn, against the canvas alone.
fn open_burst_section(name: &str, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let pages = Renderer::new(bytes.clone()).expect("renderer").page_count();
    let thumbs = pages.min(8);
    let run = |with_thumbs: bool| -> f64 {
        let svc = RenderService::with_workers(bytes.clone(), 3);
        let (tx, rx) = channel();
        let t = Instant::now();
        if with_thumbs {
            for p in 0..thumbs {
                let tx = tx.clone();
                svc.render(p, 0.285, move |_| {
                    let _ = tx.send(false);
                });
            }
        }
        for p in 0..2 {
            let tx = tx.clone();
            svc.render(p, 2.5, move |_| {
                let _ = tx.send(true);
            });
        }
        let mut got = 0;
        while got < 2 {
            if rx.recv().expect("reply") {
                got += 1;
            }
        }
        ms_since(t)
    };
    let alone = median_of(opts.reps, || run(false));
    let with = median_of(opts.reps, || run(true));
    println!(
        "opening a {name} document (3 threads): the two canvas pages are drawn after {alone:.0} ms alone, \
         {with:.0} ms behind {thumbs} thumbnails asked first\n"
    );
}

/// The calls of `ui/canvas/page.rs` for every page of the view, each time the
/// documents signal changes (scrolling to another page, a selection, a
/// zoom): `page_comment_infos` always, `annotations` with the hand tool;
/// the canvas and the status bar also ask for the page labels and the count.
fn model_section(pages: usize, opts: &Opts) {
    let bytes = long_doc(pages);
    let size = bytes.len();
    let (load, mut doc) = timed(opts.reps, || {
        Document::from_bytes(bytes.clone(), None, None).expect("load")
    });
    let (commit, _) = timed(opts.reps, || doc.commit().expect("commit"));
    println!(
        "{pages} pages ({}): Document::from_bytes {load:.1} ms, Document::commit {commit:.1} ms (both on the interface thread)",
        kib(size)
    );
    let (count, _) = timed(opts.reps, || doc.page_count());
    let (infos, n) = timed(opts.reps, || {
        (0..pages).map(|p| doc.page_comment_infos(p).len()).sum::<usize>()
    });
    let (annots, _) = timed(opts.reps, || {
        (0..pages)
            .map(|p| doc.annotations(p).map(|a| a.len()).unwrap_or(0))
            .sum::<usize>()
    });
    // Both calls find the page through `Document::page_id`, which walks the page tree.
    let (ids, _) = timed(opts.reps, || {
        (0..pages).filter(|p| doc.page_id(*p).is_ok()).count()
    });
    let (labels, _) = timed(opts.reps, || doc.page_labels());
    let (sizes, _) = timed(opts.reps, || doc.page_sizes());
    let t = Instant::now();
    let layers = doc.text_layers();
    let text_first = ms_since(t);
    let (text_again, _) = timed(opts.reps, || doc.text_layers().len());
    println!(
        "{pages} pages, {n} comments: page_count {count:.2} ms; page_comment_infos of every page {infos:.1} ms; \
         annotations of every page {annots:.1} ms (page_id of every page alone {ids:.1} ms); page_labels {labels:.2} ms; page_sizes {sizes:.2} ms; \
         text of every page (first text selection) {text_first:.0} ms ({} pages), then {text_again:.3} ms",
        layers.len()
    );
    println!(
        "  so one change of the documents signal costs about {:.0} ms of document calls on the interface thread, \
         before any diffing\n",
        infos + annots + 2.0 * labels + 4.0 * count
    );
}

// ---- generated documents ------------------------------------------------------------

/// A small random number generator (xorshift), so the documents are the same every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn rand_f32() -> f32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    let mut r = Rng(SEED.load(Ordering::Relaxed));
    let v = r.f();
    SEED.store(r.0, Ordering::Relaxed);
    v
}

/// A document with `pages` pages built from content streams, resources and
/// annotations; returns the bytes of the file.
struct Builder {
    doc: lopdf::Document,
    pages_id: lopdf::ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    fn new() -> Self {
        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        Builder {
            doc,
            pages_id,
            kids: Vec::new(),
        }
    }

    fn page(&mut self, size: (f32, f32), content: Vec<u8>, resources: Dictionary, annots: Vec<Object>) {
        let mut stream = Stream::new(Dictionary::new(), content);
        let _ = stream.compress();
        let content_id = self.doc.add_object(stream);
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => vec![0.into(), 0.into(), size.0.into(), size.1.into()],
            "Contents" => content_id,
            "Resources" => resources,
        };
        if !annots.is_empty() {
            page.set("Annots", annots);
        }
        let id = self.doc.add_object(page);
        self.kids.push(id.into());
    }

    fn image(
        &mut self,
        w: u32,
        h: u32,
        color: &str,
        bits: i64,
        filter: &str,
        data: Vec<u8>,
    ) -> lopdf::ObjectId {
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => color,
            "BitsPerComponent" => bits,
            "Filter" => filter,
        };
        self.doc.add_object(Stream::new(dict, data))
    }

    fn finish(mut self) -> Vec<u8> {
        let count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => self.kids, "Count" => count }),
        );
        let catalog = self
            .doc
            .add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        self.doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        self.doc.save_to(&mut out).expect("save");
        out
    }
}

const WORDS: [&str; 24] = [
    "document",
    "page",
    "render",
    "the",
    "of",
    "and",
    "picture",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "zoom",
    "scroll",
    "editor",
    "comment",
    "field",
    "signature",
    "layer",
    "outline",
    "text",
    "print",
];

fn line_text(rng: &mut Rng, chars: usize) -> String {
    let mut s = String::new();
    while s.len() < chars {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(WORDS[(rng.next() % WORDS.len() as u64) as usize]);
    }
    s
}

fn fonts() -> Dictionary {
    let font = |name: &str| {
        Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => name, "Encoding" => "WinAnsiEncoding"
        })
    };
    dictionary! { "F1" => font("Helvetica"), "F2" => font("Times-Roman"), "F3" => font("Helvetica-Bold") }
}

/// A4 pages of dense text in two columns (about 9000 glyphs a page).
fn text_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(7);
    for p in 0..pages {
        let mut c = format!(
            "BT /F3 15 Tf 50 800 Td (Chapter {} - {}) Tj ET\n",
            p + 1,
            line_text(&mut rng, 30)
        );
        for col in 0..2 {
            let x = 50 + col * 255;
            c.push_str(&format!("BT /F{} 8.5 Tf 10.4 TL {x} 776 Td\n", col + 1));
            for _ in 0..70 {
                c.push_str(&format!("({}) '\n", line_text(&mut rng, 58)));
            }
            c.push_str("ET\n");
        }
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => fonts() },
            vec![],
        );
    }
    b.finish()
}

/// Drawings: 2000 polylines of 8 curves each, 800 shapes filled with
/// transparency and a fine grid on every page.
fn vector_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(11);
    for _ in 0..pages {
        let mut c = String::from("0.2 w 0.8 0.8 0.85 RG\n");
        for i in 0..=120 {
            let x = 10.0 + i as f32 * 6.85;
            c.push_str(&format!("{x:.2} 10 m {x:.2} 585 l\n"));
        }
        for i in 0..=84 {
            let y = 10.0 + i as f32 * 6.85;
            c.push_str(&format!("10 {y:.2} m 832 {y:.2} l\n"));
        }
        c.push_str("S\n/GS1 gs\n");
        for _ in 0..800 {
            let (cx, cy, r) = (rng.f() * 842.0, rng.f() * 595.0, 4.0 + rng.f() * 30.0);
            c.push_str(&format!("{:.3} {:.3} {:.3} rg\n", rng.f(), rng.f(), rng.f()));
            for k in 0..6 {
                let a = k as f32 * std::f32::consts::TAU / 6.0;
                let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
                c.push_str(&format!("{x:.2} {y:.2} {}\n", if k == 0 { "m" } else { "l" }));
            }
            c.push_str("h f\n");
        }
        c.push_str("/GS0 gs\n");
        for _ in 0..2000 {
            c.push_str(&format!(
                "{:.2} w {:.3} {:.3} {:.3} RG\n",
                0.3 + rng.f(),
                rng.f(),
                rng.f(),
                rng.f()
            ));
            let (mut x, mut y) = (rng.f() * 842.0, rng.f() * 595.0);
            c.push_str(&format!("{x:.2} {y:.2} m\n"));
            for _ in 0..8 {
                let pts: Vec<f32> = (0..6).map(|_| (rng.f() - 0.5) * 40.0).collect();
                c.push_str(&format!(
                    "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n",
                    x + pts[0],
                    y + pts[1],
                    x + pts[2],
                    y + pts[3],
                    x + pts[4],
                    y + pts[5]
                ));
                x += pts[4];
                y += pts[5];
            }
            c.push_str("S\n");
        }
        let gs = dictionary! {
            "GS0" => dictionary! { "Type" => "ExtGState", "ca" => 1.0, "CA" => 1.0 },
            "GS1" => dictionary! { "Type" => "ExtGState", "ca" => 0.5 },
        };
        b.page(
            (842.0, 595.0),
            c.into_bytes(),
            dictionary! { "ExtGState" => gs },
            vec![],
        );
    }
    b.finish()
}

fn jpeg(pixels: &[u8], w: u32, h: u32, color: ExtendedColorType, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(pixels, w, h, color)
        .expect("JPEG");
    out
}

/// Landscape A4 pages, each covered by a different 4000×3000 photo (JPEG).
fn photo_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let (w, h) = (4000u32, 3000u32);
    for p in 0..pages {
        let mut rng = Rng(100 + p as u64);
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                let wave = ((fx * 23.0 + p as f32).sin() * (fy * 17.0).cos() * 0.5 + 0.5) * 120.0;
                let n = (rng.next() & 31) as f32;
                px.push((fx * 200.0 + n) as u8);
                px.push((wave + n) as u8);
                px.push((fy * 180.0 + 40.0 - n) as u8);
            }
        }
        let img = b.image(
            w,
            h,
            "DeviceRGB",
            8,
            "DCTDecode",
            jpeg(&px, w, h, ExtendedColorType::Rgb8, 85),
        );
        let content = b"q 842 0 0 595 0 0 cm /Im0 Do Q\n".to_vec();
        b.page(
            (842.0, 595.0),
            content,
            dictionary! { "XObject" => dictionary! { "Im0" => img } },
            vec![],
        );
    }
    b.finish()
}

/// Scanned A4 pages at 300 dpi (2480×3508): grey JPEG pages and a last page
/// in black and white (1 bit, Flate).
fn scan_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let (w, h) = (2480u32, 3508u32);
    for p in 0..pages {
        let mut rng = Rng(300 + p as u64);
        let mut grey = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let paper = 235 + (rng.next() % 16) as u8;
                // Lines of "words": dark blocks 40 px high every 70 px, between the margins.
                let in_line = y > 300 && y < 3300 && (y % 70) < 40 && x > 250 && x < 2230;
                let in_word = (x / 37 + y / 70 * 7) % 9 != 0 && ((x * 7 + y / 70) % 37) > 6;
                grey[(y * w + x) as usize] = if in_line && in_word {
                    30 + (rng.next() % 40) as u8
                } else {
                    paper
                };
            }
        }
        let img = if p + 1 == pages && pages > 1 {
            // Black and white: 1 bit per pixel, rows padded to bytes.
            let row = w.div_ceil(8) as usize;
            let mut bits = vec![0u8; row * h as usize];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    if grey[y * w as usize + x] > 128 {
                        bits[y * row + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut e, &bits).expect("deflate");
            b.image(w, h, "DeviceGray", 1, "FlateDecode", e.finish().expect("deflate"))
        } else {
            b.image(
                w,
                h,
                "DeviceGray",
                8,
                "DCTDecode",
                jpeg(&grey, w, h, ExtendedColorType::L8, 70),
            )
        };
        let content = b"q 595 0 0 842 0 0 cm /Im0 Do Q\n".to_vec();
        b.page(
            (595.0, 842.0),
            content,
            dictionary! { "XObject" => dictionary! { "Im0" => img } },
            vec![],
        );
    }
    b.finish()
}

/// A long document: a few lines of text and two comments (a note with its
/// pop-up and a rectangle) on every page.
fn long_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(5);
    for p in 0..pages {
        let mut c = String::from("BT /F1 11 Tf 13 TL 60 780 Td\n");
        for _ in 0..12 {
            c.push_str(&format!("({}) '\n", line_text(&mut rng, 70)));
        }
        c.push_str("ET\n");
        let popup = b.doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Popup", "Rect" => vec![400.into(), 600.into(), 580.into(), 700.into()], "Open" => false
        });
        let note = b.doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Text",
            "Rect" => vec![60.into(), 600.into(), 80.into(), 620.into()],
            "Contents" => Object::string_literal(format!("Note on page {}", p + 1)),
            "T" => Object::string_literal("Reviewer"),
            "M" => Object::string_literal("D:20261019120000Z"),
            "CreationDate" => Object::string_literal("D:20261019120000Z"),
            "Popup" => popup,
        });
        let square = b.doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Square",
            "Rect" => vec![100.into(), 400.into(), 300.into(), 500.into()],
            "C" => vec![1.into(), 0.into(), 0.into()],
            "Contents" => Object::string_literal("A rectangle"),
            "T" => Object::string_literal("Reviewer"),
        });
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => fonts() },
            vec![note.into(), popup.into(), square.into()],
        );
    }
    b.finish()
}
