//! Measures how long the pictures of the pages take, through the code path of
//! the interface: the documents are generated in memory
//! (`folitecta_core::testdocs`: text, vector drawings, photos, scanned pages,
//! embedded TrueType, CFF and composite fonts, forms, transparency, compressed
//! object streams, long documents with comments, bookmarks and links) plus
//! `docs/sample.pdf`, so no file of the user is ever read.
//!
//! ```text
//! cargo build --release -p folitecta-core --example render_bench
//! taskset -c 4-7,12-15 target/release/examples/render_bench [--quick] [--reps N] [--out DIR] [--only NAME]
//! ```
//!
//! Every time is the median of `--reps` runs (10 by default) with the first
//! and third quartiles in brackets: `12.3 [11.9–12.8]`. Run it on a quiet
//! machine, or pinned to whole physical cores: on a processor with two
//! threads per core, pin to both threads of each core used (`lscpu -e` shows
//! the pairs; `4-7,12-15` on an 8-core, 16-thread processor where 12 is the
//! second thread of core 4). The load average of the machine is printed with
//! the numbers.
//!
//! Sections: for each document (`--only text|vector|photo|scan|fonts|form|transparency|sample`)
//! opening (lopdf model and renderer parse), the time of a page split into
//! rasterizing and PNG encoding for every zoom of the interface (100, 200 and
//! 400 % at a pixel ratio of 1 and 2, and the thumbnails), tiles and low
//! resolution pictures, other ways of sending the pixels to the window
//! (encoding and decoding time, bytes) and the cost of an edit (the render
//! right after the new document reaches the render threads is cold); then
//! the render service (`--only service`: 1 to 4 threads, a burst of zoom
//! steps all at once and at the pace of a wheel, thumbnails asked before the
//! pages or already being drawn) and the document calls the interface makes
//! each time it draws the pages again (`--only model`: comments, bookmarks,
//! links, form fields, the text of the pages after an edit). `--out DIR`
//! writes the documents and the encoded pictures, for other renderers and
//! decoders.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use folitecta_core::testdocs::{
    Rng, font_doc, form_doc, long_doc_linked, photo_doc, scan_doc, text_doc, transparency_doc, vector_doc,
};
use folitecta_core::{Document, Markup, RenderService, Renderer};
use hayro::hayro_interpret::{InterpreterSettings, TransformExt};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::vello_cpu::{Pixels, Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render, render_into};
use image::ExtendedColorType;

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
        reps: 10,
        out: None,
        only: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--quick" => {
                opts.quick = true;
                opts.reps = 3;
            }
            "--reps" => opts.reps = args.next().and_then(|v| v.parse().ok()).unwrap_or(10).max(1),
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
    println!("processors available to this process: {cores}");
    if let Ok(load) = std::fs::read_to_string("/proc/loadavg") {
        println!("load average at the start: {}", load.trim());
    }
    println!(
        "times in milliseconds: median of {} runs [first quartile–third quartile]\n",
        opts.reps
    );

    let started = Instant::now();
    let wanted = |name: &str| opts.only.as_deref().is_none_or(|o| o == name);
    type Gen = Box<dyn Fn() -> Vec<u8>>;
    let quick = opts.quick;
    let mut docs: Vec<(&str, Gen)> = vec![
        ("text", Box::new(move || text_doc(if quick { 4 } else { 12 }))),
        ("vector", Box::new(move || vector_doc(if quick { 2 } else { 4 }))),
        ("photo", Box::new(|| photo_doc(2))),
        ("scan", Box::new(move || scan_doc(if quick { 2 } else { 4 }))),
        ("fonts", Box::new(move || font_doc(if quick { 2 } else { 4 }))),
        ("form", Box::new(move || form_doc(if quick { 2 } else { 4 }, 30))),
        (
            "transparency",
            Box::new(move || transparency_doc(if quick { 2 } else { 3 })),
        ),
    ];
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample.pdf");
    if let Ok(bytes) = std::fs::read(&sample) {
        docs.push(("sample", Box::new(move || bytes.clone())));
    }
    for (name, make) in &docs {
        if !wanted(name) {
            continue;
        }
        let t = Instant::now();
        let bytes = make();
        println!("## {name} (generated in {:.0} ms)", ms_since(t));
        if let Some(dir) = &opts.out {
            // The documents themselves, for other renderers and decoders.
            let _ = std::fs::write(dir.join(format!("{name}.pdf")), &bytes);
        }
        reset_peak_rss();
        let bytes = Arc::new(bytes);
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
        let fonts = Arc::new(font_doc(8));
        println!("## render service");
        service_section("text", text.clone(), &opts);
        service_section("vector", vector.clone(), &opts);
        service_section("fonts", fonts.clone(), &opts);
        zoom_burst_section("text", text.clone(), &opts);
        zoom_burst_section("vector", vector.clone(), &opts);
        open_burst_section("text", text, &opts);
        open_burst_section("vector", vector, &opts);
        if let Some(mb) = peak_rss_mb() {
            println!("peak resident memory during this section: {mb:.0} MB\n");
        }
    }
    if wanted("model") {
        println!("## document calls made each time the interface draws the pages again");
        let sizes: &[usize] = if opts.quick { &[50, 300] } else { &[50, 300, 1000] };
        for &pages in sizes {
            model_section(pages, &opts);
        }
        let forms: &[usize] = if opts.quick { &[10] } else { &[10, 50] };
        for &pages in forms {
            form_model_section(pages, &opts);
        }
    }
    if let Ok(load) = std::fs::read_to_string("/proc/loadavg") {
        println!("load average at the end: {}", load.trim());
    }
    println!("benchmark finished in {:.0} s", started.elapsed().as_secs_f64());
}

// ---- measuring ---------------------------------------------------------------------

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The median and the quartiles of repeated measures (milliseconds). It
/// prints as `median [first quartile–third quartile]`, with the precision
/// asked (`{:.1}`).
#[derive(Clone, Copy)]
struct Stat {
    med: f64,
    q1: f64,
    q3: f64,
}

impl Stat {
    fn of(v: &[f64]) -> Stat {
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        let at = |q: f64| {
            if v.is_empty() {
                return 0.0;
            }
            // Linear interpolation between the closest ranks.
            let pos = q * (v.len() - 1) as f64;
            let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
            v[lo] + (v[hi] - v[lo]) * (pos - lo as f64)
        };
        Stat {
            med: at(0.5),
            q1: at(0.25),
            q3: at(0.75),
        }
    }
}

impl std::fmt::Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p = f.precision().unwrap_or(1);
        write!(f, "{:.p$} [{:.p$}–{:.p$}]", self.med, self.q1, self.q3)
    }
}

/// `reps` runs of `f`: their times (median and quartiles) and the last result.
fn timed<T>(reps: usize, mut f: impl FnMut() -> T) -> (Stat, T) {
    let mut times = Vec::with_capacity(reps);
    let mut last = None;
    for _ in 0..reps.max(1) {
        let t = Instant::now();
        let value = f();
        times.push(ms_since(t));
        // The previous result is freed after the measure.
        last = Some(value);
    }
    (Stat::of(&times), last.expect("at least one run"))
}

/// The times `f` measures itself, `reps` times.
fn median_of(reps: usize, mut f: impl FnMut() -> f64) -> Stat {
    let v: Vec<f64> = (0..reps.max(1)).map(|_| f()).collect();
    Stat::of(&v)
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
        let raster = Stat::of(&warm);
        let mp = (px.0 as f64 * px.1 as f64) / 1e6;
        println!(
            "| {view} | {scale:.2} | {}×{} | {:.1} | {raster:.1} | {:.1} | {:.1} | {} | {} | {:.0} |",
            px.0,
            px.1,
            Stat::of(&cold),
            Stat::of(&enc),
            Stat::of(&total),
            kib(png_len),
            kib(px.0 as usize * px.1 as usize * 4),
            mp / (raster.med.max(0.001) / 1000.0)
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
        let total = Stat::of(&all);
        println!(
            "| {n} | {total:.0} ms | {:.1} | {:.0} ms |",
            pages as f64 / (total.med / 1000.0),
            Stat::of(&first)
        );
    }
    println!();
}

/// Six steps of Ctrl+wheel zoom (×1.1 each) over three visible pages: the
/// time from the last step until its pages are drawn. Three ways: every step
/// asked at once (an upper bound of what dropping stale steps saves), only
/// the last step, and the steps asked at the pace of a wheel (one every
/// 30 ms), where a render already started cannot be stopped.
fn zoom_burst_section(name: &str, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let visible = [0usize, 1, 2];
    let steps: Vec<f32> = (0..6).map(|k| 2.0 * 1.1f32.powi(k)).collect();
    let last = *steps.last().expect("steps");
    let run = |scales: &[f32], every: Duration| -> f64 {
        let svc = RenderService::with_workers(bytes.clone(), 3);
        // The threads have parsed the document and drawn the pages once.
        for p in visible {
            svc.render_blocking(p, 1.0).expect("render");
        }
        let (tx, rx) = channel();
        let mut wanted = 0;
        let mut t = Instant::now();
        for (i, s) in scales.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(every);
            }
            if *s == last {
                t = Instant::now();
            }
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
    let fifo = median_of(opts.reps, || run(&steps, Duration::ZERO));
    let paced = median_of(opts.reps, || run(&steps, Duration::from_millis(30)));
    let only_last = median_of(opts.reps, || run(&[last], Duration::ZERO));
    println!(
        "zoom burst (3 threads, 3 visible {name} pages, 6 wheel steps from scale 2.0 to {last:.2}), time from the last step to its pages: \
         every step asked at once {fifo:.0} ms, every step at the pace of a wheel (30 ms apart) {paced:.0} ms, only the last step {only_last:.0} ms\n"
    );
}

/// A document opens with the thumbnails pane shown: the pane asks for its
/// thumbnails (scale 0.285) in the same queues as the canvas, before the two
/// pages of the canvas (125 % on a pixel ratio of 2), or a moment after
/// (thumbnails already being drawn, which cannot be stopped). Time until the
/// two canvas pages are drawn, against the canvas alone.
fn open_burst_section(name: &str, bytes: Arc<Vec<u8>>, opts: &Opts) {
    let pages = Renderer::new(bytes.clone()).expect("renderer").page_count();
    let thumbs = pages.min(8);
    let run = |thumbs_first: Option<Duration>| -> f64 {
        let svc = RenderService::with_workers(bytes.clone(), 3);
        let (tx, rx) = channel();
        let mut t = Instant::now();
        if let Some(gap) = thumbs_first {
            for p in 0..thumbs {
                let tx = tx.clone();
                svc.render(p, 0.285, move |_| {
                    let _ = tx.send(false);
                });
            }
            std::thread::sleep(gap);
            t = Instant::now();
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
    let alone = median_of(opts.reps, || run(None));
    let with = median_of(opts.reps, || run(Some(Duration::ZERO)));
    let started = median_of(opts.reps, || run(Some(Duration::from_millis(10))));
    println!(
        "opening a {name} document (3 threads): the two canvas pages are drawn after {alone:.0} ms alone, \
         {with:.0} ms behind {thumbs} thumbnails asked first, {started:.0} ms when the thumbnails started 10 ms before\n"
    );
}

/// The calls of the interface each time it draws the pages again (scrolling
/// to another page, a selection, a zoom): for every page of the view
/// `page_comment_infos` always and `annotations` with the hand tool, the page
/// labels and the count; the comments pane (`comment_infos`), the bookmarks
/// pane (`outline`, with a named destination for every page) and the links
/// pane (`all_links`) when they are open. Then the text of every page, at
/// the first text selection and again after a highlight (an edit of the
/// comments only).
fn model_section(pages: usize, opts: &Opts) {
    let bytes = long_doc_linked(pages);
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
    // Both calls find the page through `Document::page_id`.
    let (ids, _) = timed(opts.reps, || {
        (0..pages).filter(|p| doc.page_id(*p).is_ok()).count()
    });
    let (labels, _) = timed(opts.reps, || doc.page_labels());
    let (sizes, _) = timed(opts.reps, || doc.page_sizes());
    let (comments, _) = timed(opts.reps, || doc.comment_infos().len());
    let (outline, items) = timed(opts.reps, || doc.outline().len());
    let (links, nlinks) = timed(opts.reps, || doc.all_links().len());
    println!(
        "  every page of the view: page_comment_infos {infos:.1} ms ({n} comments), annotations {annots:.1} ms \
         (page_id of every page alone {ids:.1} ms); page_count {count:.2} ms; page_labels {labels:.2} ms; page_sizes {sizes:.2} ms"
    );
    println!(
        "  panes: comments (comment_infos) {comments:.1} ms; bookmarks (outline, {items} items with named destinations) {outline:.1} ms; \
         links (all_links, {nlinks}) {links:.1} ms"
    );
    let t = Instant::now();
    let layers = doc.text_layers();
    let text_first = ms_since(t);
    let (text_again, _) = timed(opts.reps, || doc.text_layers().len());
    // A highlight on the first page changes only the comments.
    let quad = [60.0, 790.0, 300.0, 790.0, 60.0, 776.0, 300.0, 776.0];
    doc.add_text_markup(0, Markup::Highlight, &[quad], [1.0, 0.9, 0.2], "Reviewer", "")
        .expect("highlight");
    let t = Instant::now();
    let after = doc.text_layers().len();
    let text_after_edit = ms_since(t);
    println!(
        "  text of every page: first text selection {text_first:.0} ms ({} pages), then {text_again:.3} ms; \
         after a highlight (comments only) {text_after_edit:.0} ms ({after} pages)",
        layers.len()
    );
    println!(
        "  so one change of the documents signal costs about {:.0} ms of document calls on the interface thread \
         (panes closed), {:.0} ms with the comments, bookmarks and links panes open, before any diffing\n",
        infos.med + annots.med + 2.0 * labels.med + 4.0 * count.med,
        infos.med + annots.med + 2.0 * labels.med + 4.0 * count.med + comments.med + outline.med + links.med
    );
}

/// A form of `pages` pages with 20 fields a page: what every page of the view
/// asks with the hand tool (`page_widgets`, which reads every field) and the
/// fields pane (`form_fields`).
fn form_model_section(pages: usize, opts: &Opts) {
    let bytes = form_doc(pages, 20);
    let doc = Document::from_bytes(bytes, None, None).expect("load");
    let (fields, n) = timed(opts.reps, || doc.form_fields().len());
    let (widgets, w) = timed(opts.reps, || {
        (0..pages).map(|p| doc.page_widgets(p).len()).sum::<usize>()
    });
    println!(
        "form of {pages} pages, {n} fields: form_fields {fields:.1} ms; page_widgets of every page {widgets:.1} ms ({w} widgets)\n"
    );
}

// ---- generated documents ------------------------------------------------------------

fn rand_f32() -> f32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    let mut r = Rng(SEED.load(Ordering::Relaxed));
    let v = r.f();
    SEED.store(r.0, Ordering::Relaxed);
    v
}
