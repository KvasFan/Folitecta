//! The render threads of a document: one queue shared by every thread,
//! ordered by what the window needs first, and the pictures already made.
//!
//! - A request names the picture (page, scale, options) and the revision of
//!   the document it must show; one asked again while it is waiting or being
//!   drawn is drawn once. A request of a revision that is no longer the
//!   document's, or of a page the window no longer holds, is answered
//!   [`RenderAnswer::Stale`] without drawing.
//! - The threads take first the pages in the window (from the top down),
//!   then the pages ahead in the scroll direction (nearest first), the
//!   others, and the small pictures of the thumbnails and previews last;
//!   without a window telling what it shows, first asked first drawn.
//! - The pictures made are kept, up to a number of bytes shared by the open
//!   documents, the least recently used dropped first; a picture asked again
//!   is answered at once. When a
//!   page is drawn for the window, its small picture (at the scale the
//!   thumbnails ask for) is made from the same pixels, so thumbnails and
//!   previews of the pages seen cost no drawing.
//! - Each thread keeps its own parse of the document and render cache, made
//!   again when the document changes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use hayro::RenderCache;
use hayro::hayro_syntax::Pdf;

use crate::error::{Error, Result};
use crate::render::{RenderOpts, RenderedPage, encode_pixels, load_pdf, pixels_of, render_region};

/// The size of a tile, in pixels of the page picture: a page too large to
/// be drawn whole is drawn in tiles of this size, each one pixel larger on
/// its right and bottom edges so that neighbouring tiles overlap and no seam
/// shows between them.
pub const TILE: u32 = 512;

/// Which part of the window asks for a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// A page of a view of the document (no longer wanted once the views no
    /// longer hold the page).
    View,
    /// The small picture of a page shown while its sharp one is made (as a
    /// page of a view).
    Preview,
    /// A picture of the thumbnails pane.
    Thumb,
    /// Any other picture (the loupe, printing, a feature window): wanted
    /// until it is drawn.
    Other,
}

/// A picture asked for.
#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    pub page: usize,
    pub scale: f32,
    pub opts: RenderOpts,
    pub lane: Lane,
    /// The revision of the document the picture must show (`None`: the
    /// current one).
    pub rev: Option<u64>,
    /// A tile of the picture (column and row, [`TILE`] pixels a side), not
    /// the whole picture.
    pub tile: Option<(u32, u32)>,
}

/// What a request gets.
#[derive(Debug, Clone)]
pub enum RenderAnswer {
    Picture(Arc<RenderedPage>),
    /// No longer wanted: the document changed, or the page left the window.
    Stale,
    Failed(String),
}

/// What the render threads of a document did since they started.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Pictures asked for.
    pub requests: u64,
    /// Pictures drawn.
    pub rendered: u64,
    /// Pictures answered from those kept, without drawing.
    pub reused: u64,
    /// Requests answered without a picture because it was no longer wanted.
    pub dropped: u64,
    /// Time spent drawing and encoding, in microseconds.
    pub render_us: u64,
}

#[derive(Default)]
struct StatCounters {
    requests: AtomicU64,
    rendered: AtomicU64,
    reused: AtomicU64,
    dropped: AtomicU64,
    render_us: AtomicU64,
}

fn bump(counter: &AtomicU64, n: u64) {
    counter.fetch_add(n, Ordering::Relaxed);
}

/// The bytes of pictures all the open documents keep together (uncompressed
/// pictures: about 40 pages of text at 125 % on a pixel ratio of 2); each
/// document keeps its share, at least [`KEPT_LEAST`].
const KEPT_TOTAL: usize = 256 * 1024 * 1024;
const KEPT_LEAST: usize = 48 * 1024 * 1024;

/// Render services alive (open documents).
static OPEN: AtomicU64 = AtomicU64::new(0);

/// The bytes of pictures one document keeps.
fn kept_budget() -> usize {
    (KEPT_TOTAL / OPEN.load(Ordering::Relaxed).max(1) as usize).max(KEPT_LEAST)
}

type Reply = Box<dyn FnOnce(RenderAnswer) + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    page: usize,
    scale: u32,
    opts: RenderOpts,
    rev: u64,
    tile: Option<(u32, u32)>,
}

fn scale_key(scale: f32) -> u32 {
    (scale * 1000.0).round() as u32
}

struct Job {
    key: Key,
    scale: f32,
    lane: Lane,
    seq: u64,
}

struct Kept {
    picture: Arc<RenderedPage>,
    used: u64,
}

/// The part of the document the window shows, for the order of the work.
#[derive(Debug, Clone, Copy, Default)]
struct Focus {
    /// The pages in the window (first and last).
    shown: Option<(usize, usize)>,
    /// The pages scroll towards higher page numbers.
    forward: bool,
    /// The pages the views hold (with their pictures); a view picture of a
    /// page outside is no longer shown anywhere.
    held: Option<(usize, usize)>,
}

struct State {
    bytes: Arc<Vec<u8>>,
    /// Bumped by every new document; the threads parse it again.
    generation: u64,
    /// The revision of the document (of the pages not in `page_revs`).
    rev: u64,
    /// The revision of each page: a picture of another revision of the page
    /// is no longer wanted, one of the same revision is still good after an
    /// edit of other pages.
    page_revs: Vec<u64>,
    queue: Vec<Job>,
    seq: u64,
    /// The replies waiting for each picture asked (queued or being drawn).
    waiting: HashMap<Key, Vec<Reply>>,
    kept: HashMap<Key, Kept>,
    kept_bytes: usize,
    used: u64,
    focus: Focus,
    /// The scale of the thumbnails (milli), the size of the small pictures
    /// made with each page drawn for the window.
    thumb_scale: Option<u32>,
    /// Jobs being drawn.
    running: usize,
    stop: bool,
}

impl State {
    /// The revision of a page.
    fn rev_of(&self, page: usize) -> u64 {
        self.page_revs.get(page).copied().unwrap_or(self.rev)
    }

    fn keep(&mut self, key: Key, picture: Arc<RenderedPage>) {
        self.used += 1;
        let size = picture.png.len();
        if let Some(old) = self.kept.insert(
            key,
            Kept {
                picture,
                used: self.used,
            },
        ) {
            self.kept_bytes -= old.picture.png.len();
        }
        self.kept_bytes += size;
        let budget = kept_budget();
        while self.kept_bytes > budget && self.kept.len() > 1 {
            let Some(oldest) = self.kept.iter().min_by_key(|(_, k)| k.used).map(|(k, _)| *k) else {
                break;
            };
            if let Some(gone) = self.kept.remove(&oldest) {
                self.kept_bytes -= gone.picture.png.len();
            }
        }
    }

    fn kept(&mut self, key: &Key) -> Option<Arc<RenderedPage>> {
        self.used += 1;
        let used = self.used;
        self.kept.get_mut(key).map(|k| {
            k.used = used;
            k.picture.clone()
        })
    }

    /// Whether a job is no longer wanted.
    fn stale(&self, job: &Job) -> bool {
        if job.key.rev != self.rev_of(job.key.page) {
            return true;
        }
        match (job.lane, self.focus.held) {
            (Lane::View | Lane::Preview, Some((a, b))) => job.key.page < a || job.key.page > b,
            _ => false,
        }
    }

    /// The order of a job: lower first. Without a view telling what it
    /// shows, first asked first drawn; with one, the pages shown from the top
    /// down, then those ahead in the scroll direction (nearest first), the
    /// others, the small pictures of the pages shown, the other small
    /// pictures, the thumbnails.
    fn rank(&self, job: &Job) -> (u8, usize, u64) {
        let p = job.key.page;
        let Some((first, last)) = self.focus.shown else {
            let class = match job.lane {
                Lane::View | Lane::Other => 0,
                Lane::Preview => 3,
                Lane::Thumb => 4,
            };
            return (class, 0, job.seq);
        };
        let shown = p >= first && p <= last;
        let ahead = if self.focus.forward { p > last } else { p < first };
        let distance = if p < first {
            first - p
        } else {
            p.saturating_sub(last)
        };
        match job.lane {
            Lane::View | Lane::Other if shown => (0, p - first, job.seq),
            Lane::View if ahead => (1, distance, job.seq),
            Lane::View | Lane::Other => (2, distance, job.seq),
            Lane::Preview if shown => (3, p - first, job.seq),
            Lane::Preview => (4, distance, job.seq),
            Lane::Thumb => (5, 0, job.seq),
        }
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    stats: StatCounters,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Render threads of one document, with one queue and the pictures kept.
pub struct RenderService {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

/// Render threads per document: the processors less one, from one to four
/// (a page is drawn by one thread; the threads draw the pages of the window
/// side by side).
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .saturating_sub(1)
        .clamp(1, 4)
}

impl RenderService {
    /// Start the render threads for the given document bytes.
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self::with_workers(bytes, worker_count())
    }

    /// Start `n` render threads (at least one).
    pub fn with_workers(bytes: Arc<Vec<u8>>, n: usize) -> Self {
        OPEN.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                bytes,
                generation: 0,
                rev: 0,
                page_revs: Vec::new(),
                queue: Vec::new(),
                seq: 0,
                waiting: HashMap::new(),
                kept: HashMap::new(),
                kept_bytes: 0,
                used: 0,
                focus: Focus::default(),
                thumb_scale: None,
                running: 0,
                stop: false,
            }),
            wake: Condvar::new(),
            stats: StatCounters::default(),
        });
        let threads = (0..n.max(1))
            .map(|i| {
                let shared = shared.clone();
                std::thread::Builder::new()
                    .name(format!("folitecta-render-{i}"))
                    .spawn(move || worker(shared))
                    .expect("spawn render thread")
            })
            .collect();
        RenderService { shared, threads }
    }

    /// What the render threads did since they started.
    pub fn stats(&self) -> RenderStats {
        let s = &self.shared.stats;
        RenderStats {
            requests: s.requests.load(Ordering::Relaxed),
            rendered: s.rendered.load(Ordering::Relaxed),
            reused: s.reused.load(Ordering::Relaxed),
            dropped: s.dropped.load(Ordering::Relaxed),
            render_us: s.render_us.load(Ordering::Relaxed),
        }
    }

    /// Replace the document bytes (after an edit): the next revision.
    pub fn replace(&self, bytes: Arc<Vec<u8>>) {
        let rev = self.shared.lock().rev + 1;
        self.replace_rev(bytes, rev);
    }

    /// Replace the document bytes with those of revision `rev` (of every
    /// page). The pictures kept of other revisions are dropped, and the
    /// requests still waiting for them are answered [`RenderAnswer::Stale`].
    pub fn replace_rev(&self, bytes: Arc<Vec<u8>>, rev: u64) {
        self.update(Some(bytes), rev, Vec::new());
    }

    /// Replace the document bytes; `page_revs` holds the revision of each
    /// page: the pictures of the pages whose revision did not change stay
    /// good.
    pub fn replace_pages(&self, bytes: Arc<Vec<u8>>, page_revs: Vec<u64>) {
        let rev = page_revs.iter().copied().max().unwrap_or(0);
        self.update(Some(bytes), rev, page_revs);
    }

    /// The revision of each page changes, the document does not.
    pub fn set_page_revs(&self, page_revs: Vec<u64>) {
        let rev = page_revs.iter().copied().max().unwrap_or(0);
        self.update(None, rev, page_revs);
    }

    fn update(&self, bytes: Option<Arc<Vec<u8>>>, rev: u64, page_revs: Vec<u64>) {
        let stale = {
            let mut s = self.shared.lock();
            if let Some(bytes) = bytes {
                s.bytes = bytes;
                s.generation += 1;
            }
            s.rev = rev;
            s.page_revs = page_revs;
            let current: Vec<Key> = s.kept.keys().copied().collect();
            for k in current {
                if k.rev != s.rev_of(k.page) {
                    if let Some(gone) = s.kept.remove(&k) {
                        s.kept_bytes -= gone.picture.png.len();
                    }
                }
            }
            take_stale(&mut s)
        };
        answer_stale(&self.shared, stale);
        self.shared.wake.notify_all();
    }

    /// Tell which pages the window shows (`first` to `last`) and which way
    /// they scroll: the order of the work.
    pub fn set_focus(&self, shown: Option<(usize, usize)>, forward: bool) {
        let mut s = self.shared.lock();
        s.focus.shown = shown;
        s.focus.forward = forward;
    }

    /// Tell which pages the views hold with their pictures: a view picture
    /// of a page outside is no longer wanted (`None`: every page).
    pub fn set_held(&self, held: Option<(usize, usize)>) {
        let stale = {
            let mut s = self.shared.lock();
            s.focus.held = held;
            take_stale(&mut s)
        };
        answer_stale(&self.shared, stale);
    }

    /// Ask for a picture; `reply` runs on a render thread (or at once when
    /// the picture is kept or the request is stale).
    pub fn request(&self, req: RenderRequest, reply: impl FnOnce(RenderAnswer) + Send + 'static) {
        bump(&self.shared.stats.requests, 1);
        let mut s = self.shared.lock();
        let key = Key {
            page: req.page,
            scale: scale_key(req.scale),
            opts: req.opts,
            rev: req.rev.unwrap_or_else(|| s.rev_of(req.page)),
            tile: req.tile,
        };
        if matches!(req.lane, Lane::Thumb | Lane::Preview) {
            s.thumb_scale = Some(key.scale);
        }
        if let Some(picture) = s.kept(&key) {
            drop(s);
            bump(&self.shared.stats.reused, 1);
            reply(RenderAnswer::Picture(picture));
            return;
        }
        s.seq += 1;
        let job = Job {
            key,
            scale: req.scale,
            lane: req.lane,
            seq: s.seq,
        };
        if s.stale(&job) {
            drop(s);
            bump(&self.shared.stats.dropped, 1);
            reply(RenderAnswer::Stale);
            return;
        }
        let replies = s.waiting.entry(key).or_default();
        let new = replies.is_empty();
        replies.push(Box::new(reply));
        if new {
            s.queue.push(job);
        } else if let Some(j) = s.queue.iter_mut().find(|j| j.key == key) {
            // Asked again: as recent as the last request, in the most urgent
            // lane (and wanted as long as any of its askers wants it).
            j.seq = job.seq;
            if matches!(job.lane, Lane::View | Lane::Other) && j.lane != Lane::Other {
                j.lane = job.lane;
            }
        }
        drop(s);
        self.shared.wake.notify_one();
    }

    /// Render asynchronously; `reply` runs on the render thread.
    pub fn render(&self, page: usize, scale: f32, reply: impl FnOnce(Result<RenderedPage>) + Send + 'static) {
        self.render_ext(page, scale, RenderOpts::default(), reply);
    }

    /// Render asynchronously with explicit options.
    pub fn render_ext(
        &self,
        page: usize,
        scale: f32,
        opts: RenderOpts,
        reply: impl FnOnce(Result<RenderedPage>) + Send + 'static,
    ) {
        let req = RenderRequest {
            page,
            scale,
            opts,
            lane: Lane::Other,
            rev: None,
            tile: None,
        };
        self.request(req, move |answer| {
            reply(match answer {
                RenderAnswer::Picture(p) => Ok(p.as_ref().clone()),
                RenderAnswer::Stale => Err(Error::Render("the document changed".into())),
                RenderAnswer::Failed(e) => Err(Error::Render(e)),
            })
        });
    }

    /// Render and wait for the result.
    pub fn render_blocking(&self, page: usize, scale: f32) -> Result<RenderedPage> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.render(page, scale, move |r| {
            let _ = tx.send(r);
        });
        rx.recv()
            .map_err(|_| Error::Render("render thread stopped".into()))?
    }

    /// Page display sizes as seen by the renderer.
    pub fn page_sizes_blocking(&self) -> Result<Vec<(f32, f32)>> {
        let bytes = self.shared.lock().bytes.clone();
        let pdf = load_pdf(bytes)?;
        Ok(pdf.pages().iter().map(|p| p.render_dimensions()).collect())
    }
}

impl Drop for RenderService {
    fn drop(&mut self) {
        OPEN.fetch_sub(1, Ordering::Relaxed);
        let stale = {
            let mut s = self.shared.lock();
            s.stop = true;
            let keys: Vec<Key> = s.queue.drain(..).map(|j| j.key).collect();
            keys.into_iter()
                .flat_map(|k| s.waiting.remove(&k).unwrap_or_default())
                .collect::<Vec<_>>()
        };
        answer_stale(&self.shared, stale);
        self.shared.wake.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Take the queued jobs no longer wanted, with their replies.
fn take_stale(s: &mut State) -> Vec<Reply> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.queue.len() {
        if s.stale(&s.queue[i]) {
            let job = s.queue.swap_remove(i);
            out.extend(s.waiting.remove(&job.key).unwrap_or_default());
        } else {
            i += 1;
        }
    }
    out
}

fn answer_stale(shared: &Shared, replies: Vec<Reply>) {
    bump(&shared.stats.dropped, replies.len() as u64);
    for r in replies {
        r(RenderAnswer::Stale);
    }
}

/// Threads of the rasterizer a page drawn alone may use besides its own:
/// half the processors (about the physical cores) less one, at most four,
/// and none rather than one (one more thread costs more than it gives).
fn extra_threads() -> u16 {
    static EXTRA: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *EXTRA.get_or_init(|| {
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let extra = (n / 2).saturating_sub(1).min(4) as u16;
        if extra >= 2 { extra } else { 0 }
    })
}

/// A job taken from the queue, with what it is drawn with.
struct Taken {
    job: Job,
    generation: u64,
    thumb_scale: Option<u32>,
    /// Threads of the rasterizer besides the drawing one.
    threads: u16,
}

/// The next job to draw (the first in rank), waiting for one; `None` when
/// the service stops. The stale jobs met are answered on the way. A job
/// drawn while nothing else is drawn or waiting gets the rasterizer's
/// threads.
fn next_job(shared: &Shared) -> Option<Taken> {
    let mut s = shared.lock();
    loop {
        if s.stop {
            return None;
        }
        let stale = take_stale(&mut s);
        if !stale.is_empty() {
            drop(s);
            answer_stale(shared, stale);
            s = shared.lock();
            continue;
        }
        let best = (0..s.queue.len()).min_by_key(|&i| s.rank(&s.queue[i]));
        if let Some(i) = best {
            let job = s.queue.swap_remove(i);
            let alone = s.running == 0 && s.queue.is_empty();
            s.running += 1;
            return Some(Taken {
                job,
                generation: s.generation,
                thumb_scale: s.thumb_scale,
                threads: if alone { extra_threads() } else { 0 },
            });
        }
        s = shared.wake.wait(s).unwrap_or_else(|e| e.into_inner());
    }
}

/// The small picture of a page, from the pixels of a larger one: each pixel
/// the average of those it covers.
fn shrink(pixels: &[u8], w: u32, h: u32, channels: usize, tw: u32, th: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th) as usize * channels];
    if tw == 0 || th == 0 || w == 0 || h == 0 {
        return out;
    }
    let col_start = |x: u32| (x as u64 * w as u64 / tw as u64) as u32;
    let row_start = |y: u32| (y as u64 * h as u64 / th as u64) as u32;
    let mut sums = vec![0u32; tw as usize * channels];
    for ty in 0..th {
        let (y0, y1) = (row_start(ty), row_start(ty + 1).max(row_start(ty) + 1).min(h));
        sums.iter_mut().for_each(|v| *v = 0);
        for y in y0..y1 {
            let row =
                &pixels[(y as usize * w as usize) * channels..((y as usize + 1) * w as usize) * channels];
            for tx in 0..tw {
                let (x0, x1) = (col_start(tx), col_start(tx + 1).max(col_start(tx) + 1).min(w));
                let acc = &mut sums[tx as usize * channels..(tx as usize + 1) * channels];
                for x in x0..x1 {
                    let px = &row[x as usize * channels..(x as usize + 1) * channels];
                    for c in 0..channels {
                        acc[c] += px[c] as u32;
                    }
                }
            }
        }
        for tx in 0..tw {
            let (x0, x1) = (col_start(tx), col_start(tx + 1).max(col_start(tx) + 1).min(w));
            let n = (x1 - x0) * (y1 - y0);
            let at = (ty as usize * tw as usize + tx as usize) * channels;
            for c in 0..channels {
                out[at + c] = ((sums[tx as usize * channels + c] + n / 2) / n.max(1)) as u8;
            }
        }
    }
    out
}

fn worker(shared: Arc<Shared>) {
    let mut held: Option<Taken> = None;
    loop {
        let (generation, bytes) = {
            let s = shared.lock();
            (s.generation, s.bytes.clone())
        };
        let pdf = load_pdf(bytes);
        let cache = RenderCache::new();
        loop {
            let next = match held.take() {
                Some(j) => j,
                None => match next_job(&shared) {
                    Some(j) => j,
                    None => return,
                },
            };
            if next.generation != generation {
                // A new document: parse it first.
                held = Some(next);
                break;
            }
            draw(&shared, &pdf, &cache, next);
        }
    }
}

/// The PNG compression of a picture for the window: none up to 8 million
/// pixels (encoding and decoding the compressed picture cost as much as
/// drawing a page of text; the bytes go from one process to the other of
/// the same machine), the fast compression of fdeflate above.
fn compression(w: u32, h: u32) -> png::Compression {
    if (w as u64) * (h as u64) <= 8_000_000 {
        png::Compression::NoCompression
    } else {
        png::Compression::Fast
    }
}

/// Draw a job, answer its replies and keep the picture (and the small
/// picture of the page, made from the same pixels).
fn draw<'a>(shared: &Shared, pdf: &'a Result<Pdf>, cache: &RenderCache<'a>, taken: Taken) {
    let Taken {
        job,
        thumb_scale,
        threads,
        ..
    } = taken;
    let started = std::time::Instant::now();
    let key = job.key;
    let mut small = None;
    let answer = match pdf {
        Err(e) => RenderAnswer::Failed(e.to_string()),
        Ok(pdf) => match render_region(
            pdf,
            cache,
            key.page,
            job.scale,
            key.opts,
            key.tile.map(|(c, r)| (c * TILE, r * TILE, TILE + 1, TILE + 1)),
            threads,
        ) {
            Err(e) => RenderAnswer::Failed(e.to_string()),
            Ok(pixmap) => {
                let (w, h, color, pixels) = pixels_of(pixmap);
                match encode_pixels(w, h, color, &pixels, compression(w, h)) {
                    Err(e) => RenderAnswer::Failed(e.to_string()),
                    Ok(png) => {
                        let channels = if color == png::ColorType::Rgb { 3 } else { 4 };
                        // The small picture of the page, when it is not kept yet.
                        if let (Some(ts), Lane::View | Lane::Other, None) = (thumb_scale, job.lane, key.tile)
                        {
                            if ts < key.scale {
                                small = Some((ts, pixels, w, h, color, channels));
                            }
                        }
                        RenderAnswer::Picture(Arc::new(RenderedPage {
                            png,
                            width: w,
                            height: h,
                        }))
                    }
                }
            }
        },
    };
    bump(&shared.stats.rendered, 1);
    bump(&shared.stats.render_us, started.elapsed().as_micros() as u64);
    let replies = {
        let mut s = shared.lock();
        s.running = s.running.saturating_sub(1);
        if let RenderAnswer::Picture(p) = &answer {
            if key.rev == s.rev_of(key.page) {
                s.keep(key, p.clone());
            }
        }
        s.waiting.remove(&key).unwrap_or_default()
    };
    for r in replies {
        r(answer.clone());
    }
    // The small picture, after the answers.
    let Some((ts, pixels, w, h, color, channels)) = small else {
        return;
    };
    let Ok(pdf) = pdf else { return };
    let small_key = Key {
        scale: ts,
        tile: None,
        ..key
    };
    if shared.lock().kept.contains_key(&small_key) {
        return;
    }
    let Some(page) = pdf.pages().get(key.page) else {
        return;
    };
    // The size a render at that scale has (`render_pixmap` and hayro).
    let (pw, ph) = page.render_dimensions();
    let s = ts as f32 / 1000.0;
    let (tw, th) = ((pw * s) as u16 as u32, (ph * s) as u16 as u32);
    if tw == 0 || th == 0 || tw > w || th > h {
        return;
    }
    let shrunk = shrink(&pixels, w, h, channels, tw, th);
    let Ok(png) = encode_pixels(tw, th, color, &shrunk, compression(tw, th)) else {
        return;
    };
    let picture = Arc::new(RenderedPage {
        png,
        width: tw,
        height: th,
    });
    let replies = {
        let mut s = shared.lock();
        if key.rev != s.rev_of(key.page) {
            return;
        }
        s.keep(small_key, picture.clone());
        // Requests of it waiting in the queue are answered now.
        s.queue.retain(|j| j.key != small_key);
        s.waiting.remove(&small_key).unwrap_or_default()
    };
    bump(&shared.stats.reused, replies.len() as u64);
    for r in replies {
        r(RenderAnswer::Picture(picture.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Renderer, render_pixmap_with};

    fn doc(pages: usize) -> Arc<Vec<u8>> {
        Arc::new(crate::testdocs::text_doc(pages))
    }

    fn ask(svc: &RenderService, req: RenderRequest) -> RenderAnswer {
        let (tx, rx) = std::sync::mpsc::channel();
        svc.request(req, move |a| {
            let _ = tx.send(a);
        });
        rx.recv().unwrap()
    }

    fn req(page: usize, scale: f32, lane: Lane, rev: Option<u64>) -> RenderRequest {
        RenderRequest {
            page,
            scale,
            opts: RenderOpts::default(),
            lane,
            rev,
            tile: None,
        }
    }

    #[test]
    fn a_picture_is_that_of_a_direct_render_and_is_kept() {
        let bytes = doc(3);
        let direct = Renderer::new(bytes.clone()).unwrap().render_page(1, 0.5).unwrap();
        let svc = RenderService::with_workers(bytes, 2);
        let RenderAnswer::Picture(p) = ask(&svc, req(1, 0.5, Lane::View, None)) else {
            panic!("no picture");
        };
        // The same pixels (the picture for the window is not compressed).
        let pixels = |png: &[u8]| image::load_from_memory(png).unwrap().to_rgba8();
        assert_eq!((p.width, p.height), (direct.width, direct.height));
        assert!(pixels(&p.png) == pixels(&direct.png));
        assert!(p.png.len() > direct.png.len());
        let RenderAnswer::Picture(again) = ask(&svc, req(1, 0.5, Lane::View, None)) else {
            panic!("no picture");
        };
        assert!(Arc::ptr_eq(&p, &again));
        let st = svc.stats();
        assert_eq!((st.rendered, st.reused), (1, 1));
    }

    #[test]
    fn a_request_of_another_revision_is_stale() {
        let svc = RenderService::with_workers(doc(2), 1);
        svc.replace_rev(doc(2), 7);
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, Some(6))),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, Some(7))),
            RenderAnswer::Picture(_)
        ));
        // After a new revision, the pictures of the old one are no longer given.
        svc.replace_rev(doc(1), 8);
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, Some(7))),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, Some(8))),
            RenderAnswer::Picture(_)
        ));
        // A page that no longer exists fails, it does not hang.
        assert!(matches!(
            ask(&svc, req(5, 0.3, Lane::View, Some(8))),
            RenderAnswer::Failed(_)
        ));
    }

    #[test]
    fn the_pictures_of_pages_not_changed_stay_good() {
        let svc = RenderService::with_workers(doc(3), 1);
        svc.replace_pages(doc(3), vec![1, 1, 1]);
        for p in 0..3 {
            assert!(matches!(
                ask(&svc, req(p, 0.3, Lane::View, Some(1))),
                RenderAnswer::Picture(_)
            ));
        }
        let drawn = svc.stats().rendered;
        // An edit of page 1 only.
        svc.replace_pages(doc(3), vec![1, 2, 1]);
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, Some(1))),
            RenderAnswer::Picture(_)
        ));
        assert!(matches!(
            ask(&svc, req(2, 0.3, Lane::View, Some(1))),
            RenderAnswer::Picture(_)
        ));
        assert_eq!(svc.stats().rendered, drawn, "pages 0 and 2 are kept");
        assert!(matches!(
            ask(&svc, req(1, 0.3, Lane::View, Some(1))),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(1, 0.3, Lane::View, Some(2))),
            RenderAnswer::Picture(_)
        ));
        assert_eq!(svc.stats().rendered, drawn + 1);
    }

    #[test]
    fn pages_outside_the_views_are_not_drawn() {
        let svc = RenderService::with_workers(doc(6), 1);
        svc.set_focus(Some((2, 3)), true);
        svc.set_held(Some((1, 4)));
        assert!(matches!(
            ask(&svc, req(0, 0.3, Lane::View, None)),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(5, 0.3, Lane::Preview, None)),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(4, 0.3, Lane::View, None)),
            RenderAnswer::Picture(_)
        ));
        // Thumbnails are not part of the views.
        assert!(matches!(
            ask(&svc, req(0, 0.2, Lane::Thumb, None)),
            RenderAnswer::Picture(_)
        ));
    }

    #[test]
    fn the_same_picture_asked_twice_is_drawn_once() {
        let svc = RenderService::with_workers(doc(2), 1);
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..4 {
            let tx = tx.clone();
            svc.request(req(1, 0.6, Lane::View, None), move |a| {
                let _ = tx.send(matches!(a, RenderAnswer::Picture(_)));
            });
        }
        for _ in 0..4 {
            assert!(rx.recv().unwrap());
        }
        assert_eq!(svc.stats().rendered, 1);
    }

    #[test]
    fn thumbnails_of_pages_seen_are_made_from_their_pictures() {
        let svc = RenderService::with_workers(doc(2), 1);
        // The pane tells the scale of its thumbnails.
        assert!(matches!(
            ask(&svc, req(0, 0.2, Lane::Thumb, None)),
            RenderAnswer::Picture(_)
        ));
        assert!(matches!(
            ask(&svc, req(1, 1.0, Lane::View, None)),
            RenderAnswer::Picture(_)
        ));
        let drawn = svc.stats().rendered;
        // Made with the page: no drawing.
        let RenderAnswer::Picture(p) = ask(&svc, req(1, 0.2, Lane::Preview, None)) else {
            panic!("no thumbnail");
        };
        let direct = Renderer::new(doc(2)).unwrap().render_page(1, 0.2).unwrap();
        assert_eq!((p.width, p.height), (direct.width, direct.height));
        assert_eq!(svc.stats().rendered, drawn);
    }

    #[test]
    fn the_window_comes_first() {
        let svc = RenderService::with_workers(doc(8), 1);
        // Busy with a first page while the others are asked.
        let (tx, rx) = std::sync::mpsc::channel();
        svc.set_focus(Some((6, 6)), true);
        for p in [0usize, 1, 2, 6, 7] {
            let tx = tx.clone();
            let lane = if p == 2 { Lane::Thumb } else { Lane::View };
            svc.request(req(p, 0.4, lane, None), move |_| {
                let _ = tx.send(p);
            });
        }
        let order: Vec<usize> = (0..5).map(|_| rx.recv().unwrap()).collect();
        // The first one may have started before the others came; then the
        // page shown, the one ahead, the others, the thumbnail last.
        let rest: Vec<usize> = order.iter().copied().filter(|p| *p != order[0]).collect();
        let expected: Vec<usize> = [6usize, 7, 1, 2]
            .iter()
            .copied()
            .filter(|p| *p != order[0])
            .collect();
        assert_eq!(rest[..2], expected[..2], "{order:?}");
        assert_eq!(
            *order.last().unwrap(),
            if order[0] == 2 { 1 } else { 2 },
            "{order:?}"
        );
    }

    #[test]
    fn the_rasterizer_threads_draw_the_same_pixels() {
        use crate::testdocs::{font_doc, transparency_doc, vector_doc};
        for (name, bytes) in [
            ("text", crate::testdocs::text_doc(1)),
            ("vector", vector_doc(1)),
            ("fonts", font_doc(1)),
            ("transparency", transparency_doc(1)),
        ] {
            let pdf = load_pdf(Arc::new(bytes)).unwrap();
            let cache = RenderCache::new();
            let one = render_pixmap_with(&pdf, &cache, 0, 1.3, RenderOpts::default(), 0).unwrap();
            let four = render_pixmap_with(&pdf, &cache, 0, 1.3, RenderOpts::default(), 3).unwrap();
            assert_eq!(
                (one.width(), one.height()),
                (four.width(), four.height()),
                "{name}"
            );
            assert!(one.data() == four.data(), "{name}: the pixels differ");
        }
    }

    #[test]
    fn tiles_are_the_parts_of_the_whole_picture() {
        let bytes = doc(1);
        let svc = RenderService::with_workers(bytes.clone(), 2);
        let scale = 2.0;
        let pixels = |png: &[u8]| image::load_from_memory(png).unwrap().to_rgba8();
        let RenderAnswer::Picture(whole) = ask(&svc, req(0, scale, Lane::View, None)) else {
            panic!("no picture");
        };
        let whole = pixels(&whole.png);
        let (w, h) = whole.dimensions();
        for (c, r) in [(0u32, 0u32), (1, 1), ((w - 1) / TILE, (h - 1) / TILE)] {
            let mut tile_req = req(0, scale, Lane::View, None);
            tile_req.tile = Some((c, r));
            let RenderAnswer::Picture(tile) = ask(&svc, tile_req) else {
                panic!("no tile {c},{r}");
            };
            let tile = pixels(&tile.png);
            let (x0, y0) = (c * TILE, r * TILE);
            assert_eq!(tile.width(), (TILE + 1).min(w - x0), "tile {c},{r}");
            assert_eq!(tile.height(), (TILE + 1).min(h - y0), "tile {c},{r}");
            let mut differ = 0usize;
            for (x, y, p) in tile.enumerate_pixels() {
                let q = whole.get_pixel(x0 + x, y0 + y);
                if p.0.iter().zip(q.0.iter()).any(|(a, b)| a.abs_diff(*b) > 2) {
                    differ += 1;
                }
            }
            // The rasterizer may round the edges of shapes cut by a tile a
            // little differently; the pictures are otherwise the same.
            assert!(
                differ * 1000 < (tile.width() * tile.height()) as usize,
                "tile {c},{r}: {differ} pixels differ"
            );
        }
        // Outside the picture: refused, not hung.
        let mut beyond = req(0, scale, Lane::View, None);
        beyond.tile = Some((w / TILE + 2, 0));
        assert!(matches!(ask(&svc, beyond), RenderAnswer::Failed(_)));
    }

    #[test]
    fn shrinking_averages_the_pixels() {
        let px = vec![0u8, 0, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255];
        // 4×1 RGB to 2×1.
        assert_eq!(shrink(&px, 4, 1, 3, 2, 1), vec![128, 128, 128, 128, 128, 128]);
    }
}
