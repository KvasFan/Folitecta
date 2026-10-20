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
//! - The pictures made are kept within one budget of bytes for every open
//!   document together: past it the least recently used picture of any
//!   document is dropped first, and a document's pictures go when it
//!   closes; a picture asked again is answered at once. When a
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
/// pictures: about 20 pages of text at 125 % on a pixel ratio of 2, 85 at
/// 100 % on a pixel ratio of 1). The pages a view holds keep their pictures
/// in the page; these are for the pages seen again later.
const KEPT_TOTAL: usize = 128 * 1024 * 1024;

/// The pictures kept by the render threads of every open document together,
/// within one budget: past it, the least recently used picture of any
/// document is dropped first (the newest stays, even alone larger than the
/// budget), and the pictures of a document go when it closes.
pub(crate) struct KeptPool {
    budget: usize,
    inner: Mutex<KeptPictures>,
}

#[derive(Default)]
struct KeptPictures {
    /// The pictures by render service and picture.
    pictures: HashMap<(u64, Key), Kept>,
    bytes: usize,
    used: u64,
}

impl KeptPool {
    pub(crate) fn new(budget: usize) -> Self {
        KeptPool {
            budget,
            inner: Mutex::new(KeptPictures::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, KeptPictures> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keep a picture of the render service `id`.
    fn keep(&self, id: u64, key: Key, picture: Arc<RenderedPage>) {
        let mut k = self.lock();
        k.used += 1;
        let used = k.used;
        let size = picture.png.len();
        if let Some(old) = k.pictures.insert((id, key), Kept { picture, used }) {
            k.bytes -= old.picture.png.len();
        }
        k.bytes += size;
        while k.bytes > self.budget && k.pictures.len() > 1 {
            let Some(oldest) = k.pictures.iter().min_by_key(|(_, p)| p.used).map(|(key, _)| *key) else {
                break;
            };
            if let Some(gone) = k.pictures.remove(&oldest) {
                k.bytes -= gone.picture.png.len();
            }
        }
    }

    /// A picture kept of the render service `id`, now the most recently used.
    fn get(&self, id: u64, key: &Key) -> Option<Arc<RenderedPage>> {
        let mut k = self.lock();
        k.used += 1;
        let used = k.used;
        k.pictures.get_mut(&(id, *key)).map(|p| {
            p.used = used;
            p.picture.clone()
        })
    }

    fn contains(&self, id: u64, key: &Key) -> bool {
        self.lock().pictures.contains_key(&(id, *key))
    }

    /// Keep only the pictures of the render service `id` that `keep` accepts.
    fn retain(&self, id: u64, keep: impl Fn(&Key) -> bool) {
        let mut k = self.lock();
        let KeptPictures { pictures, bytes, .. } = &mut *k;
        pictures.retain(|(i, key), p| {
            let stays = *i != id || keep(key);
            if !stays {
                *bytes -= p.picture.png.len();
            }
            stays
        });
    }

    /// The bytes of the pictures kept.
    fn bytes(&self) -> usize {
        self.lock().bytes
    }
}

/// The pool of the pictures kept by the render services of the program.
fn kept_pool() -> Arc<KeptPool> {
    static POOL: std::sync::OnceLock<Arc<KeptPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| Arc::new(KeptPool::new(KEPT_TOTAL))).clone()
}

/// The bytes of the pictures the render threads of every open document keep
/// together (what the memory of the program holds for them).
pub fn kept_bytes() -> usize {
    kept_pool().bytes()
}

/// What the program does when the render threads of a document have drawn
/// and have nothing more to do, and when a document closes (on Linux, give
/// the memory freed back to the system): set once by [`on_idle`].
static IDLE_HOOK: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

/// Set what the program does when the render threads of a document go idle
/// after drawing or a document closes. It runs on a render thread (or the
/// thread closing the document) and must be quick; set once, later calls
/// are ignored.
pub fn on_idle(hook: fn()) {
    let _ = IDLE_HOOK.set(hook);
}

fn idle() {
    if let Some(hook) = IDLE_HOOK.get() {
        hook();
    }
}

/// How long a picture that failed fails at once when asked again.
const FAILED_FOR: std::time::Duration = std::time::Duration::from_secs(10);

/// The number of the next render service (the key of its pictures kept).
static NEXT_SERVICE: AtomicU64 = AtomicU64::new(1);

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
    /// One of its askers wants it whatever the views hold (a thumbnail, a
    /// picture for another use): it stays when the views move.
    wanted_outside: bool,
}

/// The askers of a picture.
#[derive(Default)]
struct Waiting {
    replies: Vec<Reply>,
    /// One of them wants it whatever the views hold (a thumbnail, a picture
    /// for another use): known whether the job is queued or already taken.
    wanted_outside: bool,
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
    /// The askers waiting for each picture (queued, taken or being drawn).
    waiting: HashMap<Key, Waiting>,
    /// The pictures that failed lately, with why: asked again (the page
    /// script tries a picture again after an error) they fail at once
    /// instead of being drawn again.
    failed: HashMap<Key, (std::time::Instant, String)>,
    /// How long a failure is answered at once ([`FAILED_FOR`]).
    failed_for: std::time::Duration,
    focus: Focus,
    /// The pages each view (of a split window: two) holds with their
    /// pictures, as the views draw them: a view picture of a page that no
    /// view holds is no longer shown anywhere (no view told: every page).
    views: [Option<(usize, usize)>; 2],
    /// The scale of the thumbnails (milli), the size of the small pictures
    /// made with each page drawn for the window.
    thumb_scale: Option<u32>,
    /// Jobs being drawn.
    running: usize,
    stop: bool,
    /// Pictures were drawn since the threads last had nothing to do.
    drawn_since_idle: bool,
}

impl State {
    /// The revision of a page.
    fn rev_of(&self, page: usize) -> u64 {
        self.page_revs.get(page).copied().unwrap_or(self.rev)
    }

    /// Whether a job is no longer wanted.
    fn stale(&self, job: &Job) -> bool {
        if job.key.rev != self.rev_of(job.key.page) {
            return true;
        }
        let page = job.key.page;
        let told = self.views.iter().any(Option::is_some);
        let held = self.views.iter().flatten().any(|&(a, b)| page >= a && page <= b);
        // Wanted whatever the views hold by its first asker or by any asker
        // since, whether the job is queued or already taken.
        let wanted_outside =
            job.wanted_outside || self.waiting.get(&job.key).is_some_and(|w| w.wanted_outside);
        matches!(job.lane, Lane::View | Lane::Preview) && !wanted_outside && told && !held
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
    /// The number of this service in the pool of the pictures kept.
    id: u64,
    pool: Arc<KeptPool>,
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
        Self::with_pool(bytes, n, kept_pool())
    }

    /// Start `n` render threads keeping their pictures in `pool`.
    pub(crate) fn with_pool(bytes: Arc<Vec<u8>>, n: usize, pool: Arc<KeptPool>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                bytes,
                generation: 0,
                rev: 0,
                page_revs: Vec::new(),
                queue: Vec::new(),
                seq: 0,
                waiting: HashMap::new(),
                failed: HashMap::new(),
                failed_for: FAILED_FOR,
                focus: Focus::default(),
                views: [None; 2],
                thumb_scale: None,
                running: 0,
                stop: false,
                drawn_since_idle: false,
            }),
            wake: Condvar::new(),
            stats: StatCounters::default(),
            id: NEXT_SERVICE.fetch_add(1, Ordering::Relaxed),
            pool,
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
                // A failure may be one of the bytes (unreadable), not of the
                // page: new bytes are tried again.
                s.failed.clear();
            }
            s.rev = rev;
            s.page_revs = page_revs;
            self.shared
                .pool
                .retain(self.shared.id, |k| k.rev == s.rev_of(k.page));
            let failed = std::mem::take(&mut s.failed);
            s.failed = failed
                .into_iter()
                .filter(|(k, _)| k.rev == s.rev_of(k.page))
                .collect();
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

    /// Tell which pages view `view` (0, or 1 in a split window) holds with
    /// their pictures (`None`: none), when it draws them and before their
    /// pictures are asked for: a view picture of a page that no view holds
    /// is no longer wanted.
    pub fn set_held_view(&self, view: usize, held: Option<(usize, usize)>) {
        let stale = {
            let mut s = self.shared.lock();
            let Some(slot) = s.views.get_mut(view) else {
                return;
            };
            if *slot == held {
                return;
            }
            *slot = held;
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
        if let Some(picture) = self.shared.pool.get(self.shared.id, &key) {
            drop(s);
            bump(&self.shared.stats.reused, 1);
            reply(RenderAnswer::Picture(picture));
            return;
        }
        if let Some((at, why)) = s.failed.get(&key) {
            if at.elapsed() < s.failed_for {
                let why = why.clone();
                drop(s);
                reply(RenderAnswer::Failed(why));
                return;
            }
        }
        s.seq += 1;
        let job = Job {
            key,
            scale: req.scale,
            lane: req.lane,
            seq: s.seq,
            wanted_outside: matches!(req.lane, Lane::Thumb | Lane::Other),
        };
        if s.stale(&job) {
            drop(s);
            bump(&self.shared.stats.dropped, 1);
            reply(RenderAnswer::Stale);
            return;
        }
        let waiting = s.waiting.entry(key).or_default();
        let new = waiting.replies.is_empty();
        waiting.replies.push(Box::new(reply));
        waiting.wanted_outside |= job.wanted_outside;
        if new {
            s.queue.push(job);
        } else if let Some(j) = s.queue.iter_mut().find(|j| j.key == key) {
            // Asked again: as recent as the last request, in the most urgent
            // lane (and wanted as long as any of its askers wants it: a
            // thumbnail asked with the small picture of a page in a view
            // stays when the view moves on).
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
        let stale = {
            let mut s = self.shared.lock();
            s.stop = true;
            let keys: Vec<Key> = s.queue.drain(..).map(|j| j.key).collect();
            keys.into_iter()
                .flat_map(|k| s.waiting.remove(&k).map(|w| w.replies).unwrap_or_default())
                .collect::<Vec<_>>()
        };
        answer_stale(&self.shared, stale);
        self.shared.wake.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        // The document closed: its pictures leave the pool.
        self.shared.pool.retain(self.shared.id, |_| false);
        idle();
    }
}

/// Take the queued jobs no longer wanted, with their replies.
fn take_stale(s: &mut State) -> Vec<Reply> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.queue.len() {
        if s.stale(&s.queue[i]) {
            let job = s.queue.swap_remove(i);
            out.extend(s.waiting.remove(&job.key).map(|w| w.replies).unwrap_or_default());
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
        // Nothing more to draw after drawing: the program may give the
        // memory freed back to the system.
        if s.running == 0 && s.drawn_since_idle {
            s.drawn_since_idle = false;
            drop(s);
            idle();
            s = shared.lock();
            continue;
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

/// A job taken for an older document than the one a thread parsed, decided
/// under the lock: answered as no longer wanted (its replies returned) when
/// its page changed, no asker wants it any longer or the service stops;
/// otherwise kept, to be drawn from the document just read.
fn settle_taken(s: &mut State, held: &mut Option<Taken>) -> Vec<Reply> {
    let Some(taken) = held.as_mut() else {
        return Vec::new();
    };
    if s.stop || s.stale(&taken.job) {
        let key = taken.job.key;
        *held = None;
        s.running = s.running.saturating_sub(1);
        return s.waiting.remove(&key).map(|w| w.replies).unwrap_or_default();
    }
    taken.generation = s.generation;
    Vec::new()
}

fn worker(shared: Arc<Shared>) {
    let mut held: Option<Taken> = None;
    loop {
        // The document to draw from. A job taken for an older one is decided
        // here, under the lock, so that the document changing again meanwhile
        // cannot leave it waiting for ever: answered as no longer wanted when
        // its page changed (or the service stops), else drawn from this one.
        // Whether the service stops is read in the same hold of the lock as
        // the decision about the job set aside (which answers it then): read
        // apart, the service could stop in between and the job kept would be
        // left with its askers unanswered.
        let (generation, bytes, dropped, stop) = {
            let mut s = shared.lock();
            let dropped = settle_taken(&mut s, &mut held);
            (s.generation, s.bytes.clone(), dropped, s.stop)
        };
        answer_stale(&shared, dropped);
        if stop {
            return;
        }
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
        s.drawn_since_idle = true;
        match &answer {
            RenderAnswer::Picture(p) if key.rev == s.rev_of(key.page) => {
                shared.pool.keep(shared.id, key, p.clone());
            }
            RenderAnswer::Failed(why) => {
                let failed_for = s.failed_for;
                s.failed.retain(|_, (at, _)| at.elapsed() < failed_for);
                s.failed.insert(key, (std::time::Instant::now(), why.clone()));
            }
            _ => {}
        }
        s.waiting.remove(&key).map(|w| w.replies).unwrap_or_default()
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
    if shared.pool.contains(shared.id, &small_key) {
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
        shared.pool.keep(shared.id, small_key, picture.clone());
        // Requests of it waiting in the queue are answered now.
        s.queue.retain(|j| j.key != small_key);
        s.waiting
            .remove(&small_key)
            .map(|w| w.replies)
            .unwrap_or_default()
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
        svc.set_held_view(0, Some((1, 4)));
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

    /// Sets its flag when dropped (a test that fails stops its helper thread).
    struct StopOnDrop(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn quick_changes_of_the_document_leave_no_request_unanswered() {
        use std::time::{Duration, Instant};
        let bytes = doc(3);
        let svc = Arc::new(RenderService::with_workers(bytes.clone(), 4));
        let stop = StopOnDrop(Arc::new(std::sync::atomic::AtomicBool::new(false)));
        // The document changes twice in a row, again and again (quick edits,
        // the layers made default), while pictures are taken and drawn.
        let changer = {
            let (svc, stop, bytes) = (svc.clone(), stop.0.clone(), bytes.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    svc.replace(bytes.clone());
                    svc.replace(bytes.clone());
                    std::thread::sleep(Duration::from_micros(300));
                }
            })
        };
        let started = Instant::now();
        let mut n = 0usize;
        while started.elapsed() < Duration::from_secs(3) {
            let (tx, rx) = std::sync::mpsc::channel();
            svc.request(
                req(n % 3, 0.2 + (n % 7) as f32 * 0.01, Lane::Other, None),
                move |a| {
                    let _ = tx.send(matches!(a, RenderAnswer::Picture(_)));
                },
            );
            assert!(
                rx.recv_timeout(Duration::from_secs(5)).is_ok(),
                "request {n} not answered"
            );
            n += 1;
        }
        drop(stop);
        changer.join().unwrap();
        // Closing the document returns at once.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(Arc::try_unwrap(svc).ok());
            let _ = tx.send(());
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "closing the document does not return"
        );
    }

    #[test]
    fn the_pictures_kept_stay_within_one_budget_for_every_document() {
        // A pool of its own, of about five pictures of a page at 50 %.
        let budget = 2 * 1024 * 1024;
        let pool = Arc::new(KeptPool::new(budget));
        let bytes = doc(6);
        let a = RenderService::with_pool(bytes.clone(), 1, pool.clone());
        let b = RenderService::with_pool(bytes, 1, pool.clone());
        // Whether the picture was drawn (not kept), and its bytes.
        let drawn = |svc: &RenderService, page: usize| {
            let before = svc.stats().rendered;
            let RenderAnswer::Picture(p) = ask(svc, req(page, 0.5, Lane::Other, None)) else {
                panic!("no picture of page {page}");
            };
            (svc.stats().rendered != before, p.png.len())
        };
        for page in 0..6 {
            drawn(&a, page);
            assert!(pool.bytes() <= budget, "{} bytes kept", pool.bytes());
        }
        for page in 0..6 {
            drawn(&b, page);
            assert!(pool.bytes() <= budget, "{} bytes kept", pool.bytes());
        }
        // The least recently used pictures went first, whatever their
        // document: all those of the first one.
        let (again, size) = drawn(&a, 5);
        assert!(again, "a picture of the first document still kept");
        assert!(!drawn(&b, 5).0, "the pictures used last are kept");
        // A document closed gives its pictures back.
        drop(b);
        assert_eq!(
            pool.bytes(),
            size,
            "only the picture of the first document is left"
        );
        drop(a);
        assert_eq!(pool.bytes(), 0);
    }

    #[test]
    fn a_thumbnail_asked_with_a_page_preview_stays_wanted() {
        let svc = RenderService::with_workers(doc(6), 1);
        let (tx, rx) = std::sync::mpsc::channel();
        // The thread busy with a large picture while the others are asked.
        let busy = tx.clone();
        svc.request(req(0, 4.0, Lane::View, None), move |a| {
            let _ = busy.send((0, matches!(a, RenderAnswer::Picture(_))));
        });
        // The canvas asks for the small picture of page 5, the thumbnails
        // pane for the same picture.
        let preview = tx.clone();
        svc.request(req(5, 0.2, Lane::Preview, None), move |a| {
            let _ = preview.send((1, matches!(a, RenderAnswer::Picture(_))));
        });
        svc.request(req(5, 0.2, Lane::Thumb, None), move |a| {
            let _ = tx.send((2, matches!(a, RenderAnswer::Picture(_))));
        });
        // The views no longer hold page 5; the pane still shows it.
        svc.set_held_view(0, Some((0, 2)));
        let mut got: Vec<(u8, bool)> = (0..3).map(|_| rx.recv().unwrap()).collect();
        got.sort();
        assert_eq!(got, vec![(0, true), (1, true), (2, true)]);
    }

    /// The pictures a store keeps and their bytes, checking that the bytes
    /// it counts are those of its pictures.
    fn store_of(pool: &KeptPool) -> (usize, usize) {
        let k = pool.lock();
        let bytes: usize = k.pictures.values().map(|p| p.picture.png.len()).sum();
        assert_eq!(bytes, k.bytes, "the bytes counted are not those of the pictures");
        (k.pictures.len(), k.bytes)
    }

    #[test]
    fn under_every_change_each_request_is_answered_and_the_store_holds() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        use std::time::{Duration, Instant};
        let budget = 3 * 1024 * 1024;
        let pool = Arc::new(KeptPool::new(budget));
        let (eight, six) = (doc(8), doc(6));
        let services: Vec<Arc<RenderService>> = (0..3)
            .map(|_| Arc::new(RenderService::with_pool(eight.clone(), 3, pool.clone())))
            .collect();
        let stop = StopOnDrop(Arc::new(AtomicBool::new(false)));
        let (asked, answered) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let mut threads = Vec::new();
        // Each document changes in every way, at random.
        for (n, svc) in services.iter().enumerate() {
            let (svc, stop, eight, six) = (svc.clone(), stop.0.clone(), eight.clone(), six.clone());
            threads.push(std::thread::spawn(move || {
                let mut r = crate::testdocs::Rng(0x9E37_79B9 + n as u64);
                let mut below = |n: u64| r.next_u64() % n;
                let mut rev = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    match below(5) {
                        0 => svc.replace(if below(2) == 0 { eight.clone() } else { six.clone() }),
                        1 | 2 => {
                            rev += 1;
                            let revs: Vec<u64> = (0..8).map(|_| rev.saturating_sub(below(3))).collect();
                            if below(2) == 0 {
                                svc.replace_pages(eight.clone(), revs);
                            } else {
                                svc.set_page_revs(revs);
                            }
                        }
                        3 => {
                            let a = below(8) as usize;
                            let held = (below(4) != 0).then(|| (a, (a + below(3) as usize).min(7)));
                            svc.set_held_view(below(3) as usize, held);
                        }
                        _ => {
                            let a = below(8) as usize;
                            svc.set_focus(Some((a, a)), below(2) == 0);
                        }
                    }
                    std::thread::sleep(Duration::from_micros(200 + below(800)));
                }
            }));
        }
        // Pictures of every lane, tiles, revisions and pages past the end.
        for n in 0..4 {
            let (services, stop) = (services.clone(), stop.0.clone());
            let (asked, answered) = (asked.clone(), answered.clone());
            threads.push(std::thread::spawn(move || {
                let mut r = crate::testdocs::Rng(0xDEAD_BEEF + n as u64);
                let mut below = |n: u64| r.next_u64() % n;
                while !stop.load(Ordering::Relaxed) {
                    let svc = &services[below(services.len() as u64) as usize];
                    let lane = [Lane::View, Lane::Preview, Lane::Thumb, Lane::Other][below(4) as usize];
                    let mut asked_for = req(
                        below(9) as usize,
                        [0.1f32, 0.2, 0.3, 1.0][below(4) as usize],
                        lane,
                        None,
                    );
                    asked_for.rev = (below(3) == 0).then(|| below(20));
                    asked_for.tile = (below(5) == 0).then(|| (below(2) as u32, below(2) as u32));
                    asked.fetch_add(1, Ordering::SeqCst);
                    let answered = answered.clone();
                    svc.request(asked_for, move |_| {
                        answered.fetch_add(1, Ordering::SeqCst);
                    });
                    if below(4) == 0 {
                        std::thread::sleep(Duration::from_micros(below(2000)));
                    }
                }
            }));
        }
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(1500) {
            std::thread::sleep(Duration::from_millis(50));
            let (n, bytes) = store_of(&pool);
            assert!(bytes <= budget || n <= 1, "{bytes} bytes kept in {n} pictures");
        }
        drop(stop);
        for t in threads {
            t.join().unwrap();
        }
        // Once the changes stop, every request is answered.
        let started = Instant::now();
        while answered.load(Ordering::SeqCst) < asked.load(Ordering::SeqCst)
            && started.elapsed() < Duration::from_secs(20)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(asked.load(Ordering::SeqCst), answered.load(Ordering::SeqCst));
        // Each document closes at once and gives its pictures back.
        for svc in services {
            let started = Instant::now();
            drop(Arc::try_unwrap(svc).ok().expect("the only owner"));
            assert!(started.elapsed() < Duration::from_secs(5));
            store_of(&pool);
        }
        assert_eq!(store_of(&pool), (0, 0));
    }

    #[test]
    fn a_document_closed_with_requests_in_flight_answers_them_all() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        use std::time::{Duration, Instant};
        for round in 0..10 {
            let pool = Arc::new(KeptPool::new(64 * 1024 * 1024));
            let svc = Arc::new(RenderService::with_pool(doc(6), 4, pool.clone()));
            let answered = Arc::new(AtomicUsize::new(0));
            let stop = StopOnDrop(Arc::new(AtomicBool::new(false)));
            let changer = {
                let (svc, stop) = (svc.clone(), stop.0.clone());
                std::thread::spawn(move || {
                    let bytes = doc(6);
                    while !stop.load(Ordering::Relaxed) {
                        svc.replace(bytes.clone());
                        svc.replace(bytes.clone());
                        std::thread::sleep(Duration::from_micros(100));
                    }
                })
            };
            for i in 0..200 {
                let answered = answered.clone();
                svc.request(
                    req(i % 6, 0.3 + (i % 5) as f32 * 0.1, Lane::Other, None),
                    move |_| {
                        answered.fetch_add(1, Ordering::SeqCst);
                    },
                );
            }
            std::thread::sleep(Duration::from_millis(round % 7));
            drop(stop);
            changer.join().unwrap();
            let started = Instant::now();
            drop(Arc::try_unwrap(svc).ok().expect("the only owner"));
            assert!(started.elapsed() < Duration::from_secs(5), "round {round}");
            assert_eq!(answered.load(Ordering::SeqCst), 200, "round {round}");
            assert_eq!(store_of(&pool), (0, 0), "round {round}");
        }
    }

    #[test]
    fn a_store_too_small_for_one_picture_still_answers_every_page() {
        let pool = Arc::new(KeptPool::new(1));
        let svc = RenderService::with_pool(doc(10), 2, pool.clone());
        svc.set_held_view(0, Some((0, 9)));
        svc.set_focus(Some((0, 9)), true);
        for round in 0..3 {
            for page in 0..10 {
                assert!(
                    matches!(
                        ask(&svc, req(page, 1.0, Lane::View, None)),
                        RenderAnswer::Picture(_)
                    ),
                    "round {round}, page {page}"
                );
            }
        }
        // It keeps the last picture alone.
        assert_eq!(store_of(&pool).0, 1);
    }

    #[test]
    fn a_thumbnail_asked_while_its_job_is_set_aside_stays_wanted() {
        let svc = RenderService::with_workers(doc(6), 1);
        let (tx, rx) = std::sync::mpsc::channel();
        let preview = tx.clone();
        let mut s = svc.shared.lock();
        // A thread took the small picture of page 5 for the canvas, then saw
        // that the document changed and set it aside to parse it again.
        let key = Key {
            page: 5,
            scale: scale_key(0.2),
            opts: RenderOpts::default(),
            rev: s.rev_of(5),
            tile: None,
        };
        s.waiting.entry(key).or_default().replies.push(Box::new(move |a| {
            let _ = preview.send((1, matches!(a, RenderAnswer::Stale)));
        }));
        s.running += 1;
        let mut held = Some(Taken {
            job: Job {
                key,
                scale: 0.2,
                lane: Lane::Preview,
                seq: 1,
                wanted_outside: false,
            },
            generation: s.generation,
            thumb_scale: None,
            threads: 0,
        });
        drop(s);
        // Meanwhile the thumbnails pane asks for the same picture, and the
        // views move away from page 5.
        svc.request(req(5, 0.2, Lane::Thumb, None), move |a| {
            let _ = tx.send((2, matches!(a, RenderAnswer::Stale)));
        });
        svc.set_held_view(0, Some((0, 2)));
        // The thread decides the job: still wanted, by the thumbnail.
        let mut s = svc.shared.lock();
        let dropped = settle_taken(&mut s, &mut held);
        let kept = held.is_some();
        s.running -= usize::from(kept);
        let left = s.waiting.remove(&key);
        drop(s);
        assert!(dropped.is_empty() && kept, "the job set aside was dropped");
        assert_eq!(left.map(|w| w.replies.len()), Some(2));
        assert!(rx.try_recv().is_err(), "an asker was answered no longer wanted");
    }

    #[test]
    fn a_picture_that_fails_is_drawn_once_for_its_retries() {
        let svc = RenderService::with_workers(doc(3), 1);
        // Page 8 of 3, and a tile outside the picture: each fails.
        let mut tile = req(0, 0.5, Lane::View, None);
        tile.tile = Some((40, 0));
        for asked in [req(7, 0.5, Lane::View, None), tile] {
            let before = svc.stats().rendered;
            // The page script asks again three times.
            for _ in 0..4 {
                assert!(matches!(ask(&svc, asked), RenderAnswer::Failed(_)));
            }
            assert_eq!(svc.stats().rendered, before + 1, "{asked:?}");
        }
        // Another revision of the document is tried again.
        svc.replace(doc(3));
        let before = svc.stats().rendered;
        assert!(matches!(
            ask(&svc, req(7, 0.5, Lane::View, None)),
            RenderAnswer::Failed(_)
        ));
        assert_eq!(svc.stats().rendered, before + 1);
    }

    /// The times the idle hook ran (one hook for the whole process).
    static IDLE_CALLS: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn the_program_hears_when_the_threads_go_idle_and_when_a_document_closes() {
        on_idle(|| {
            IDLE_CALLS.fetch_add(1, Ordering::SeqCst);
        });
        let svc = RenderService::with_workers(doc(3), 2);
        let before = IDLE_CALLS.load(Ordering::SeqCst);
        // A burst of pictures: told once it is drawn (other tests' services
        // may tell too, so at least once).
        let (tx, rx) = std::sync::mpsc::channel();
        for p in 0..3 {
            let tx = tx.clone();
            svc.request(req(p, 0.4, Lane::Other, None), move |_| {
                let _ = tx.send(());
            });
        }
        for _ in 0..3 {
            rx.recv().unwrap();
        }
        let started = std::time::Instant::now();
        while IDLE_CALLS.load(Ordering::SeqCst) == before && started.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            IDLE_CALLS.load(Ordering::SeqCst) > before,
            "not told after the burst"
        );
        // Pictures kept are answered without drawing: no new burst.
        let after_burst = IDLE_CALLS.load(Ordering::SeqCst);
        drop(svc);
        assert!(
            IDLE_CALLS.load(Ordering::SeqCst) > after_burst,
            "not told when it closed"
        );
    }

    #[test]
    fn a_failure_of_unreadable_bytes_ends_with_good_bytes() {
        let good = doc(3);
        let svc = RenderService::with_workers(good.clone(), 1);
        svc.replace_pages(good.clone(), vec![5, 5, 5]);
        // An edit of page 3 gives bytes that cannot be read: page 1, which
        // did not change, fails with them.
        svc.replace_pages(Arc::new(b"%PDF-1.7\nnot a document".to_vec()), vec![5, 5, 6]);
        assert!(matches!(
            ask(&svc, req(0, 0.5, Lane::Other, None)),
            RenderAnswer::Failed(_)
        ));
        // Undone: the bytes are good again and page 1 still at revision 5.
        svc.replace_pages(good, vec![5, 5, 7]);
        assert!(matches!(
            ask(&svc, req(0, 0.5, Lane::Other, None)),
            RenderAnswer::Picture(_)
        ));
    }

    #[test]
    fn a_picture_that_failed_is_tried_again_after_a_while() {
        let svc = RenderService::with_workers(doc(3), 1);
        svc.shared.lock().failed_for = std::time::Duration::from_millis(60);
        let before = svc.stats().rendered;
        for _ in 0..3 {
            assert!(matches!(
                ask(&svc, req(7, 0.5, Lane::View, None)),
                RenderAnswer::Failed(_)
            ));
        }
        assert_eq!(svc.stats().rendered, before + 1);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(matches!(
            ask(&svc, req(7, 0.5, Lane::View, None)),
            RenderAnswer::Failed(_)
        ));
        assert_eq!(
            svc.stats().rendered,
            before + 2,
            "tried again once the time passed"
        );
    }

    #[test]
    fn a_document_closed_right_after_new_bytes_answers_every_request() {
        use std::sync::atomic::AtomicUsize;
        let bytes = doc(4);
        for round in 0..200u64 {
            let svc = RenderService::with_workers(bytes.clone(), 8);
            let answered = Arc::new(AtomicUsize::new(0));
            for i in 0..40 {
                let answered = answered.clone();
                svc.request(req(i % 4, 0.2 + i as f32 * 0.01, Lane::Other, None), move |_| {
                    answered.fetch_add(1, Ordering::SeqCst);
                });
            }
            // New bytes of the same revision (an undo of an edit of another
            // page, the layers made default): the threads set their jobs
            // aside to parse them, and the document closes meanwhile.
            svc.replace_rev(bytes.clone(), 0);
            std::thread::sleep(std::time::Duration::from_micros(250 * (round % 12)));
            drop(svc);
            assert_eq!(answered.load(Ordering::SeqCst), 40, "round {round}");
        }
    }

    #[test]
    fn the_pages_held_are_those_of_every_view() {
        let svc = RenderService::with_workers(doc(12), 1);
        svc.set_held_view(0, Some((5, 9)));
        assert!(matches!(
            ask(&svc, req(2, 0.3, Lane::View, None)),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(7, 0.3, Lane::View, None)),
            RenderAnswer::Picture(_)
        ));
        // The other view of a split window holds pages 0 to 3.
        svc.set_held_view(1, Some((0, 3)));
        assert!(matches!(
            ask(&svc, req(2, 0.3, Lane::View, None)),
            RenderAnswer::Picture(_)
        ));
        // The first view jumped to page 11: its old pages are no longer held.
        svc.set_held_view(0, Some((10, 11)));
        assert!(matches!(
            ask(&svc, req(7, 0.31, Lane::View, None)),
            RenderAnswer::Stale
        ));
        assert!(matches!(
            ask(&svc, req(11, 0.3, Lane::View, None)),
            RenderAnswer::Picture(_)
        ));
        // Neither view holds anything: no page is refused.
        svc.set_held_view(0, None);
        svc.set_held_view(1, None);
        assert!(matches!(
            ask(&svc, req(7, 0.32, Lane::View, None)),
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
    fn tiles_of_a_large_page_at_a_high_scale_are_where_the_views_put_them() {
        use crate::testdocs::Builder;
        // A0 at 600 % (the page view) and A1 at 800 % (the loupe at its
        // largest): pictures wider than the largest whole picture drawn.
        for ((w, h), scale) in [((2384.0f32, 3370.0f32), 6.0f32), ((1684.0, 2384.0), 8.0)] {
            // A black square of 20 pt, its top left corner 1400 pt from the
            // left and 2300 pt from the top of the page.
            let (left, top, side) = (1400.0f32, 2300.0f32, 20.0f32);
            let mut b = Builder::new();
            let content = format!("0 g {left} {} {side} {side} re f", h - top - side);
            b.page((w, h), content.into_bytes(), lopdf::Dictionary::new(), Vec::new());
            let svc = RenderService::with_workers(Arc::new(b.finish()), 2);
            // The grid the views lay out over the picture at that scale.
            let (pw, ph) = ((w * scale) as u32, (h * scale) as u32);
            let (cols, rows) = (pw.div_ceil(TILE), ph.div_ceil(TILE));
            let tile = |c: u32, r: u32| {
                let mut tile_req = req(0, scale, Lane::Other, None);
                tile_req.tile = Some((c, r));
                match ask(&svc, tile_req) {
                    RenderAnswer::Picture(p) => image::load_from_memory(&p.png).unwrap().to_rgba8(),
                    other => panic!("tile {c},{r} of {cols}x{rows} at {scale}: {other:?}"),
                }
            };
            // Every tile of the grid has its size, the last ones too.
            for (c, r) in [(0, 0), (cols - 1, 0), (0, rows - 1), (cols - 1, rows - 1)] {
                let t = tile(c, r);
                assert_eq!(
                    t.width(),
                    (TILE + 1).min(pw - c * TILE),
                    "tile {c},{r} at {scale}"
                );
                assert_eq!(
                    t.height(),
                    (TILE + 1).min(ph - r * TILE),
                    "tile {c},{r} at {scale}"
                );
            }
            // The square is where the grid puts it: its middle black, just
            // outside it white.
            let at = |x: f32, y: f32| {
                let (x, y) = ((x * scale) as u32, (y * scale) as u32);
                let t = tile(x / TILE, y / TILE);
                t.get_pixel(x % TILE, y % TILE).0
            };
            let black = at(left + side / 2.0, top + side / 2.0);
            assert!(
                black[..3].iter().all(|v| *v < 40),
                "middle of the square at {scale}: {black:?}"
            );
            let white = at(left - 4.0, top + side / 2.0);
            assert!(
                white[..3].iter().all(|v| *v > 215),
                "left of the square at {scale}: {white:?}"
            );
            let white = at(left + side / 2.0, top + side + 4.0);
            assert!(
                white[..3].iter().all(|v| *v > 215),
                "under the square at {scale}: {white:?}"
            );
        }
    }

    #[test]
    fn shrinking_averages_the_pixels() {
        let px = vec![0u8, 0, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255];
        // 4×1 RGB to 2×1.
        assert_eq!(shrink(&px, 4, 1, 3, 2, 1), vec![128, 128, 128, 128, 128, 128]);
    }
}
