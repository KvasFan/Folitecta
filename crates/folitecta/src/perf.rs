//! Counters of the interface's own work: how often the parts of the window
//! are drawn again and how the page pictures are asked for and answered.
//! They are plain atomic counters, always on (an addition each), read by the
//! performance step of the self-test.

use std::sync::atomic::{AtomicU64, Ordering};

/// One counter.
pub struct Counter(AtomicU64);

impl Counter {
    pub const fn new() -> Self {
        Counter(AtomicU64::new(0))
    }

    pub fn bump(&self) {
        self.add(1);
    }

    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Renders of the components that follow the documents.
pub static PAGE_VIEWS: Counter = Counter::new();
pub static CANVASES: Counter = Counter::new();
pub static THUMBNAIL_PANES: Counter = Counter::new();
pub static STATUS_BARS: Counter = Counter::new();
pub static RIBBONS: Counter = Counter::new();

/// Page pictures (`/page/…`): requests received, answered with a picture,
/// answered otherwise (an error, a page that no longer exists, a revision
/// or scale no longer shown), bytes sent and the time from the request to
/// the answer (microseconds, summed).
pub static PAGE_REQUESTS: Counter = Counter::new();
pub static PAGE_PICTURES: Counter = Counter::new();
pub static PAGE_OTHER: Counter = Counter::new();
pub static PAGE_BYTES: Counter = Counter::new();
pub static PAGE_WAIT_US: Counter = Counter::new();

/// The counters at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Snapshot {
    pub page_views: u64,
    pub canvases: u64,
    pub thumbnail_panes: u64,
    pub status_bars: u64,
    pub ribbons: u64,
    pub page_requests: u64,
    pub page_pictures: u64,
    pub page_other: u64,
    pub page_bytes: u64,
    pub page_wait_us: u64,
}

impl Snapshot {
    pub fn now() -> Self {
        Snapshot {
            page_views: PAGE_VIEWS.get(),
            canvases: CANVASES.get(),
            thumbnail_panes: THUMBNAIL_PANES.get(),
            status_bars: STATUS_BARS.get(),
            ribbons: RIBBONS.get(),
            page_requests: PAGE_REQUESTS.get(),
            page_pictures: PAGE_PICTURES.get(),
            page_other: PAGE_OTHER.get(),
            page_bytes: PAGE_BYTES.get(),
            page_wait_us: PAGE_WAIT_US.get(),
        }
    }

    /// What happened since `before`.
    pub fn since(self, before: Snapshot) -> Snapshot {
        Snapshot {
            page_views: self.page_views - before.page_views,
            canvases: self.canvases - before.canvases,
            thumbnail_panes: self.thumbnail_panes - before.thumbnail_panes,
            status_bars: self.status_bars - before.status_bars,
            ribbons: self.ribbons - before.ribbons,
            page_requests: self.page_requests - before.page_requests,
            page_pictures: self.page_pictures - before.page_pictures,
            page_other: self.page_other - before.page_other,
            page_bytes: self.page_bytes - before.page_bytes,
            page_wait_us: self.page_wait_us - before.page_wait_us,
        }
    }
}
